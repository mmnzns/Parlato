// Parlato : dictees longues avec Parakeet TDT, decoupees en fenetres.
//
// Une seule passe sur un long enregistrement (147 s chez Florian) fait deriver
// Parakeet v3 vers l'anglais et lui fait sauter des phrases entieres. Au-dela
// de 15 s on decoupe donc l'audio en fenetres qui se recouvrent, on decode
// chacune, puis on recolle les jetons (tokens) horodates.
//
// Reference VoiceInk : Transcription/Core/FluidAudioTranscriptionService.swift
// (appelle AsrManager.transcribe, qui passe par ChunkProcessor des que
// l'audio depasse 15 s).
// Reference FluidAudio : Sources/FluidAudio/ASR/Parakeet/SlidingWindow/TDT/
// ChunkProcessor.swift @ 762baf67 (le commit que VoiceInk epingle) et
// TokenDeduplication/SequenceMatcher.swift. Premier portage Rust (plus simple) :
// Florian (Parla) 8050a42.
//
// Ce qui est porte : fenetres de 14,96 s avec 2 s de recouvrement, debut de
// fenetre cale sur le silence, derniere fenetre prolongee vers l'arriere avec
// de l'audio reel, fusion de deux fenetres voisines par appariement des jetons
// (suite contigue, sinon plus longue sous-suite commune, sinon coupe au
// milieu), garde de frontiere de mot, horodatages jamais decroissants, doublon
// "have Have" retire a la couture.
// Ce qui ne l'est pas (suites possibles) : la reparation des trous de 1,5 s et
// plus (re-decodage), le decodage des fenetres en parallele, le recalage des
// horodatages de couture du 09-10. Les fenetres passent l'une apres l'autre sur
// la session chargee.
// Ecarts connus avec FluidAudio : les nombres de la v3 servent pour tous les
// modeles TDT (FluidAudio reserve du contexte a gauche pour la v2 seulement) ;
// le garde "ne pas comprimer la fin d'une phrase" s'applique toujours (chez
// FluidAudio il ne sert que si une fenetre de rechauffe est utilisee, ce qui
// n'est pas le cas ici) ; la derniere fenetre se termine des que son audio
// atteint la fin de la parole (et non la fin du fichier).
//
// Unites : echantillons 16 kHz ; une trame de l'encodeur = 1 280 echantillons
// (80 ms). Tout est en fonctions pures, testees sans modele.

use anyhow::Result;
use parakeet_rs::TimedToken;

pub const SAMPLE_RATE: usize = 16_000;
/// Une trame de l'encodeur (80 ms).
const FRAME_SAMPLES: usize = 1_280;
const FRAME_SECS: f32 = 0.08;
/// Plus longue entree acceptee en une passe (FluidAudio
/// `ASRConstants.maxModelSamples`, 15 s). En dessous, rien ne change.
pub const SINGLE_PASS_MAX_SAMPLES: usize = 240_000;
/// Pas de la STFT (`melHopSize`) retire de la fenetre pour rester sous la limite.
const MEL_HOP_SAMPLES: usize = 160;
/// Fenetre : 187 trames = 239 360 echantillons (14,96 s).
const WINDOW_SAMPLES: usize =
    (SINGLE_PASS_MAX_SAMPLES - MEL_HOP_SAMPLES) / FRAME_SAMPLES * FRAME_SAMPLES;
/// Recouvrement : 2,0 s = 25 trames.
const OVERLAP_SAMPLES: usize = 32_000 / FRAME_SAMPLES * FRAME_SAMPLES;
const OVERLAP_SECS: f32 = 2.0;
/// Pas prevu entre deux debuts de fenetre (12,96 s).
const STRIDE_SAMPLES: usize = (WINDOW_SAMPLES - OVERLAP_SAMPLES) / FRAME_SAMPLES * FRAME_SAMPLES;
/// Recouvrement minimal garanti entre deux fenetres (6 trames, 480 ms).
const MIN_OVERLAP_SAMPLES: usize = 6 * FRAME_SAMPLES;
/// Recherche d'un silence franc : +-4 s autour du debut prevu.
const SILENCE_RADIUS_FRAMES: usize = 4 * SAMPLE_RATE / FRAME_SAMPLES;
/// Recherche d'un creux : +-0,5 s.
const VALLEY_RADIUS_FRAMES: usize = SAMPLE_RATE / 2 / FRAME_SAMPLES;
/// RMS en dessous duquel une trame n'est plus de la parole (~-66 dBFS).
const SPEECH_RMS_FLOOR: f32 = 0.0005;
/// Deux jetons se correspondent s'ils different de moins de 1 s.
const MATCH_TOLERANCE_SECS: f32 = OVERLAP_SECS / 2.0;

/// Fenetre `[start, end)` donnee au modele. Les jetons qui commencent avant
/// `own_from` viennent d'audio emprunte a la fenetre precedente (derniere
/// fenetre prolongee vers l'arriere) : ils sont deja dans le resultat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window {
    pub start: usize,
    pub end: usize,
    pub own_from: usize,
}

// ---------------------------------------------------------------------------
// Planification des fenetres
// ---------------------------------------------------------------------------

/// Energie moyenne (somme des carres / nombre) de `[centre - 1 trame, centre +
/// 1 trame]`, bornee a l'audio.
fn boundary_energy(samples: &[f32], center: usize) -> f32 {
    let start = center.saturating_sub(FRAME_SAMPLES);
    let end = (center + FRAME_SAMPLES).min(samples.len());
    if end <= start {
        return 0.0;
    }
    let sum: f32 = samples[start..end].iter().map(|s| s * s).sum();
    sum / (end - start) as f32
}

/// Meilleur debut candidat autour de `target_frame` : (debut, score, mediane).
struct Candidate {
    start: usize,
    score: f32,
    median: f32,
}

fn best_boundary(
    samples: &[f32],
    target_frame: usize,
    radius_frames: usize,
    previous_start: usize,
    latest_covered: usize,
) -> Candidate {
    let n = samples.len();
    let target_start = (target_frame * FRAME_SAMPLES)
        .max(previous_start + FRAME_SAMPLES)
        .min(latest_covered);
    let lower = target_frame.saturating_sub(radius_frames).max(1);
    let upper = ((n - 1) / FRAME_SAMPLES).min(target_frame + radius_frames);
    let mut best_start = target_start;
    let mut best_score = f32::MAX;
    let mut scores: Vec<f32> = Vec::new();
    if lower <= upper {
        for frame in lower..=upper {
            let candidate = frame * FRAME_SAMPLES;
            if candidate <= previous_start || candidate > latest_covered {
                continue;
            }
            let score = boundary_energy(samples, candidate);
            scores.push(score);
            if score < best_score {
                best_score = score;
                best_start = candidate;
            }
        }
    }
    if scores.is_empty() {
        return Candidate {
            start: target_start,
            score: f32::MAX,
            median: 0.0,
        };
    }
    scores.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    Candidate {
        start: best_start,
        score: best_score,
        median: scores[scores.len() / 2],
    }
}

fn threshold(median: f32, ratio: f32) -> f32 {
    if median > 0.0 {
        median * ratio
    } else {
        0.0
    }
}

/// Reculer le debut forcerait la coupe suivante dans de la parole : le point
/// prevu et le point force ont tous deux plus de 80 % de l'energie mediane.
fn would_compress_speech_tail(
    samples: &[f32],
    candidate_start: usize,
    target_start: usize,
    median: f32,
) -> bool {
    if median <= 0.0 {
        return false;
    }
    let forced_next = candidate_start + WINDOW_SAMPLES - MIN_OVERLAP_SAMPLES;
    if forced_next >= samples.len() {
        return false;
    }
    let speech_like = median * 0.8;
    boundary_energy(samples, target_start) > speech_like
        && boundary_energy(samples, forced_next) > speech_like
}

/// Debuts de fenetre (FluidAudio `silenceAlignedChunkStarts`). Chaque cible est
/// `k * pas` ; le debut retenu est le point le plus calme voisin.
fn silence_aligned_starts(samples: &[f32]) -> Vec<usize> {
    let n = samples.len();
    let mut starts = vec![0usize];
    let mut previous = 0usize;
    let mut target = STRIDE_SAMPLES;
    while target < n {
        let target_frame = target / FRAME_SAMPLES;
        let latest_covered = previous + WINDOW_SAMPLES - MIN_OVERLAP_SAMPLES;
        let target_start = (target_frame * FRAME_SAMPLES)
            .max(previous + FRAME_SAMPLES)
            .min(latest_covered);

        let silence = best_boundary(
            samples,
            target_frame,
            SILENCE_RADIUS_FRAMES,
            previous,
            latest_covered,
        );
        let mut best = if silence.score <= threshold(silence.median, 0.05) {
            let compresses = silence.start < target_start
                && would_compress_speech_tail(samples, silence.start, target_start, silence.median);
            if compresses {
                target_start
            } else {
                silence.start
            }
        } else {
            let valley = best_boundary(
                samples,
                target_frame,
                VALLEY_RADIUS_FRAMES,
                previous,
                latest_covered,
            );
            if valley.score <= threshold(valley.median, 0.35) {
                valley.start
            } else {
                target_start
            }
        };
        if best <= previous {
            best = (previous + STRIDE_SAMPLES).min(n);
        }
        starts.push(best);
        previous = best;
        target += STRIDE_SAMPLES;
    }
    starts
}

/// Fin de la parole : la fin de l'audio moins les trames finales dont le RMS
/// est sous le plancher. Toute l'audio si tout est sous le plancher.
fn speech_end_samples(samples: &[f32]) -> usize {
    let mut end = samples.len();
    while end > 0 {
        let frame_start = end.saturating_sub(FRAME_SAMPLES);
        let frame = &samples[frame_start..end];
        let sum: f32 = frame.iter().map(|s| s * s).sum();
        if (sum / frame.len() as f32).sqrt() >= SPEECH_RMS_FLOOR {
            return end;
        }
        end = frame_start;
    }
    samples.len()
}

/// Audio reel a emprunter derriere une derniere fenetre trop courte, pour
/// qu'elle fasse une fenetre pleine qui se termine a la fin de la parole.
fn last_window_backfill(chunk_start: usize, speech_end: usize) -> usize {
    let is_last = chunk_start + WINDOW_SAMPLES >= speech_end;
    let remaining = speech_end.saturating_sub(chunk_start);
    if !is_last || remaining == 0 || chunk_start == 0 {
        return 0;
    }
    let fill = WINDOW_SAMPLES.saturating_sub(remaining) / FRAME_SAMPLES * FRAME_SAMPLES;
    if fill == 0 {
        return 0;
    }
    let available = chunk_start / FRAME_SAMPLES * FRAME_SAMPLES;
    available.min(fill)
}

/// Decoupe `samples` (plus de 15 s) en fenetres. Une fenetre dont le debut est
/// apres la fin de la parole est ignoree.
pub fn plan_windows(samples: &[f32]) -> Vec<Window> {
    let n = samples.len();
    if n == 0 {
        return Vec::new();
    }
    let starts = silence_aligned_starts(samples);
    let speech_end = speech_end_samples(samples);
    let mut windows = Vec::new();
    let mut index = 0usize;
    let mut chunk_start = starts[0];
    while chunk_start < n {
        let backfill = last_window_backfill(chunk_start, speech_end);
        let visible = WINDOW_SAMPLES.saturating_sub(backfill).max(FRAME_SAMPLES);
        let candidate_end = chunk_start + visible;
        let is_last = candidate_end >= speech_end;
        let audio_end = if is_last {
            n.min(speech_end)
        } else {
            candidate_end
        };
        if audio_end <= chunk_start {
            break;
        }
        windows.push(Window {
            start: chunk_start - backfill,
            end: audio_end,
            own_from: chunk_start,
        });
        if is_last {
            break;
        }
        index += 1;
        chunk_start = if index < starts.len() {
            starts[index]
        } else {
            chunk_start + STRIDE_SAMPLES
        };
    }
    windows
}

// ---------------------------------------------------------------------------
// Fusion des jetons
// ---------------------------------------------------------------------------

fn tok_end(t: &TimedToken) -> f32 {
    t.start + FRAME_SECS
}

fn starts_word(text: &str) -> bool {
    text.starts_with(char::is_whitespace)
}

/// Un jeton peut ouvrir une couture sans coller deux mots : debut de mot, ou
/// ponctuation / symbole seul (FluidAudio `isSpliceSafePiece`).
fn splice_safe(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    starts_word(text)
        || text
            .chars()
            .all(|c| !c.is_alphanumeric() && !c.is_whitespace())
}

fn same_token(a: &TimedToken, b: &TimedToken) -> bool {
    a.text.to_lowercase() == b.text.to_lowercase()
        && (a.start - b.start).abs() < MATCH_TOLERANCE_SECS
}

/// Plus longue suite de correspondances consecutives (indices dans les deux
/// listes). Les premieres rencontrees gagnent a egalite.
fn contiguous_matches(left: &[&TimedToken], right: &[&TimedToken]) -> Vec<(usize, usize)> {
    let mut best: Vec<(usize, usize)> = Vec::new();
    for i in 0..left.len() {
        for j in 0..right.len() {
            if !same_token(left[i], right[j]) {
                continue;
            }
            let mut run = Vec::new();
            let (mut k, mut l) = (i, j);
            while k < left.len() && l < right.len() && same_token(left[k], right[l]) {
                run.push((k, l));
                k += 1;
                l += 1;
            }
            if run.len() > best.len() {
                best = run;
            }
        }
    }
    best
}

/// Plus longue sous-suite commune (programmation dynamique).
fn longest_common_subsequence(left: &[&TimedToken], right: &[&TimedToken]) -> Vec<(usize, usize)> {
    let (ln, rn) = (left.len(), right.len());
    let mut dp = vec![vec![0usize; rn + 1]; ln + 1];
    for i in 1..=ln {
        for j in 1..=rn {
            dp[i][j] = if same_token(left[i - 1], right[j - 1]) {
                dp[i - 1][j - 1] + 1
            } else {
                dp[i - 1][j].max(dp[i][j - 1])
            };
        }
    }
    let mut out = Vec::new();
    let (mut i, mut j) = (ln, rn);
    while i > 0 && j > 0 {
        if same_token(left[i - 1], right[j - 1]) {
            out.push((i - 1, j - 1));
            i -= 1;
            j -= 1;
        } else if dp[i - 1][j] > dp[i][j - 1] {
            i -= 1;
        } else {
            j -= 1;
        }
    }
    out.reverse();
    out
}

/// Fusionne deux fenetres voisines (FluidAudio `mergeChunks`). Les jetons sont
/// deja en temps absolu.
fn merge_chunks(left: Vec<TimedToken>, right: Vec<TimedToken>) -> Vec<TimedToken> {
    if left.is_empty() {
        return right;
    }
    if right.is_empty() {
        return left;
    }
    let left_end = tok_end(&left[left.len() - 1]);
    let right_start = right[0].start;
    if left_end <= right_start {
        let mut out = left;
        out.extend(right);
        return out;
    }

    // Jetons du recouvrement : indices dans `left` / `right`.
    let overlap_left: Vec<usize> = (0..left.len())
        .filter(|&i| tok_end(&left[i]) > right_start - OVERLAP_SECS)
        .collect();
    let overlap_right: Vec<usize> = (0..right.len())
        .filter(|&j| right[j].start < left_end + OVERLAP_SECS)
        .collect();
    if overlap_left.len() < 2 || overlap_right.len() < 2 {
        return merge_by_midpoint(left, right, left_end, right_start);
    }

    let l_refs: Vec<&TimedToken> = overlap_left.iter().map(|&i| &left[i]).collect();
    let r_refs: Vec<&TimedToken> = overlap_right.iter().map(|&j| &right[j]).collect();
    let minimum_pairs = (overlap_left.len() / 2).max(1);

    let mut pairs = contiguous_matches(&l_refs, &r_refs);
    if pairs.len() < minimum_pairs {
        pairs = longest_common_subsequence(&l_refs, &r_refs);
        if pairs.is_empty() {
            return merge_by_midpoint(left, right, left_end, right_start);
        }
    }
    let matches: Vec<(usize, usize)> = pairs
        .iter()
        .map(|&(a, b)| (overlap_left[a], overlap_right[b]))
        .collect();
    merge_using_matches(&matches, &left, &right)
}

fn merge_using_matches(
    matches: &[(usize, usize)],
    left: &[TimedToken],
    right: &[TimedToken],
) -> Vec<TimedToken> {
    let mut result: Vec<TimedToken> = Vec::new();
    let first_left = matches[0].0;
    result.extend_from_slice(&left[..first_left]);

    for (idx, &(li, ri)) in matches.iter().enumerate() {
        result.push(left[li].clone());
        let Some(&(next_li, next_ri)) = matches.get(idx + 1) else {
            continue;
        };
        let gap_left = if next_li > li + 1 {
            &left[li + 1..next_li]
        } else {
            &[][..]
        };
        let gap_right = if next_ri > ri + 1 {
            &right[ri + 1..next_ri]
        } else {
            &[][..]
        };
        if gap_right.len() > gap_left.len() {
            result.extend_from_slice(gap_right);
        } else {
            result.extend_from_slice(gap_left);
        }
    }

    let (last_li, last_ri) = matches[matches.len() - 1];
    if last_ri + 1 < right.len() {
        let tail = &right[last_ri + 1..];
        if !splice_safe(&tail[0].text) {
            // La couture tombe au milieu d'un mot : un seul cote doit le decouper.
            let word_start = (0..=last_ri).rev().find(|&k| splice_safe(&right[k].text));
            let adopted = match word_start {
                Some(ws) if pop_seam_word(&mut result) => {
                    // La droite a entendu le mot des son debut : on adopte son decoupage.
                    result.extend_from_slice(&right[ws..]);
                    true
                }
                _ => false,
            };
            if !adopted {
                // La droite commence au milieu du mot : la gauche le termine,
                // la droite reprend au mot suivant.
                let mut cursor = last_li + 1;
                while cursor < left.len() && !splice_safe(&left[cursor].text) {
                    result.push(left[cursor].clone());
                    cursor += 1;
                }
                match tail.iter().position(|t| splice_safe(&t.text)) {
                    Some(resume) => result.extend_from_slice(&tail[resume..]),
                    // Aucun debut de mot : mieux vaut un collage que perdre un mot.
                    None => result.extend_from_slice(tail),
                }
            }
        } else {
            result.extend_from_slice(tail);
        }
    }
    result
}

/// Retire de `result` le dernier mot (jusqu'au dernier jeton qui ouvre un mot
/// ou une ponctuation). Faux, et rien de touche, s'il n'y en a pas.
fn pop_seam_word(result: &mut Vec<TimedToken>) -> bool {
    match (0..result.len())
        .rev()
        .find(|&k| splice_safe(&result[k].text))
    {
        Some(at) => {
            result.truncate(at);
            true
        }
        None => false,
    }
}

/// Coupe au milieu du recouvrement (FluidAudio `mergeByMidpoint`).
fn merge_by_midpoint(
    left: Vec<TimedToken>,
    right: Vec<TimedToken>,
    left_end: f32,
    right_start: f32,
) -> Vec<TimedToken> {
    let cutoff = (left_end + right_start) / 2.0;
    let mut left_cut = left
        .iter()
        .position(|t| t.start >= cutoff)
        .unwrap_or(left.len());
    let mut right_from = right
        .iter()
        .position(|t| t.start >= cutoff)
        .unwrap_or(right.len());
    // Un mot ne doit pas etre coupe en deux : la gauche finit son mot, la
    // droite saute les morceaux de mot orphelins.
    if left_cut > 0 {
        while left_cut < left.len() && !splice_safe(&left[left_cut].text) {
            left_cut += 1;
        }
    }
    if let Some(offset) = right[right_from..]
        .iter()
        .position(|t| splice_safe(&t.text))
    {
        right_from += offset;
    }
    let mut out = left[..left_cut].to_vec();
    out.extend_from_slice(&right[right_from..]);
    out
}

/// Les horodatages ne reculent jamais, sans reordonner les jetons : l'ordre de
/// la fusion fait foi pour le texte.
fn enforce_monotonic(tokens: &mut [TimedToken]) {
    let mut last = f32::MIN;
    for t in tokens.iter_mut() {
        if t.start < last {
            t.start = last;
        } else {
            last = t.start;
        }
        if t.end < t.start {
            t.end = t.start;
        }
    }
}

struct Word {
    range: std::ops::Range<usize>,
    core: String,
    start: f32,
    ends_sentence: bool,
}

/// Retire un mot en double qui ne differe que par les majuscules ("have
/// Have"), pres d'une couture. `seams` : (debut de la fenetre de droite, fin
/// de celle de gauche) pour chaque fusion.
fn collapse_seam_duplicates(tokens: Vec<TimedToken>, seams: &[(f32, f32)]) -> Vec<TimedToken> {
    if tokens.len() < 2 || seams.is_empty() {
        return tokens;
    }
    let mut words: Vec<Word> = Vec::new();
    for (i, t) in tokens.iter().enumerate() {
        match words.last_mut() {
            Some(w) if !starts_word(&t.text) => w.range.end = i + 1,
            _ => words.push(Word {
                range: i..i + 1,
                core: String::new(),
                start: t.start,
                ends_sentence: false,
            }),
        }
    }
    for w in &mut words {
        let text: String = tokens[w.range.clone()]
            .iter()
            .map(|t| t.text.as_str())
            .collect();
        let trimmed = text.trim();
        w.ends_sentence = trimmed.chars().last().is_some_and(|c| ".?!:".contains(c));
        w.core = trimmed
            .trim_matches(|c: char| !c.is_alphanumeric())
            .to_string();
    }

    let near_seam = |t: f32| {
        seams
            .iter()
            .any(|&(lo, hi)| t >= lo - OVERLAP_SECS && t <= hi + OVERLAP_SECS)
    };
    let mut keep = vec![true; words.len()];
    let mut last_kept: Option<usize> = None;
    for index in 0..words.len() {
        let Some(prev_i) = last_kept else {
            last_kept = Some(index);
            continue;
        };
        let (prev, cur) = (&words[prev_i], &words[index]);
        let duplicate = !prev.core.is_empty()
            && !cur.core.is_empty()
            && prev.core != cur.core
            && prev.core.to_lowercase() == cur.core.to_lowercase()
            && cur.core.chars().next().is_some_and(char::is_alphabetic)
            && !prev.ends_sentence
            && cur.start - prev.start <= OVERLAP_SECS
            && near_seam(cur.start);
        if !duplicate {
            last_kept = Some(index);
            continue;
        }
        // On garde la copie en minuscules ; sinon la plus ancienne.
        if cur.core == cur.core.to_lowercase() && prev.core != prev.core.to_lowercase() {
            keep[prev_i] = false;
            last_kept = Some(index);
        } else {
            keep[index] = false;
        }
    }

    let mut out = Vec::with_capacity(tokens.len());
    for (w, keep) in words.iter().zip(keep) {
        if keep {
            out.extend_from_slice(&tokens[w.range.clone()]);
        }
    }
    out
}

/// Fusionne les fenetres dans l'ordre (jetons en temps absolu).
pub fn merge_windows(windows: Vec<Vec<TimedToken>>) -> Vec<TimedToken> {
    let mut iter = windows.into_iter();
    let Some(mut merged) = iter.next() else {
        return Vec::new();
    };
    let mut seams: Vec<(f32, f32)> = Vec::new();
    for next in iter {
        if let (Some(last), Some(first)) = (merged.last(), next.first()) {
            seams.push((first.start, tok_end(last)));
        }
        merged = merge_chunks(merged, next);
    }
    enforce_monotonic(&mut merged);
    collapse_seam_duplicates(merged, &seams)
}

/// Texte d'une suite de jetons, comme parakeet-rs pour une passe unique :
/// les morceaux SentencePiece collés, un espace de tete ouvre un mot.
pub fn tokens_to_text(tokens: &[TimedToken]) -> String {
    tokens
        .iter()
        .map(|t| t.text.as_str())
        .collect::<String>()
        .trim()
        .to_string()
}

/// Decode `samples` (plus de 15 s) fenetre par fenetre. `decode` recoit une
/// tranche d'audio et rend les jetons, en secondes relatives a la tranche.
pub fn transcribe_in_windows<F>(samples: &[f32], mut decode: F) -> Result<(String, usize)>
where
    F: FnMut(&[f32]) -> Result<Vec<TimedToken>>,
{
    let windows = plan_windows(samples);
    let mut outputs: Vec<Vec<TimedToken>> = Vec::with_capacity(windows.len());
    for w in &windows {
        let tokens = decode(&samples[w.start..w.end])?;
        let offset = w.start as f32 / SAMPLE_RATE as f32;
        let own_from = w.own_from as f32 / SAMPLE_RATE as f32;
        // Les jetons de l'audio emprunte (avant own_from) sont deja dans la
        // fenetre precedente.
        let kept: Vec<TimedToken> = tokens
            .into_iter()
            .map(|t| TimedToken {
                text: t.text,
                start: t.start + offset,
                end: t.end + offset,
            })
            .filter(|t| t.start >= own_from - 1e-3)
            .collect();
        outputs.push(kept);
    }
    let count = outputs.len();
    Ok((tokens_to_text(&merge_windows(outputs)), count))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tok(text: &str, start: f32) -> TimedToken {
        TimedToken {
            text: text.to_string(),
            start,
            end: start + FRAME_SECS,
        }
    }

    /// Jetons d'un mot par trame de 0,08 s a partir de `start`.
    fn words(list: &[&str], start: f32, step: f32) -> Vec<TimedToken> {
        list.iter()
            .enumerate()
            .map(|(i, w)| tok(&format!(" {w}"), start + i as f32 * step))
            .collect()
    }

    #[test]
    fn constants_follow_fluidaudio() {
        assert_eq!(WINDOW_SAMPLES, 187 * FRAME_SAMPLES);
        assert_eq!(WINDOW_SAMPLES, 239_360);
        assert_eq!(OVERLAP_SAMPLES, 32_000);
        assert_eq!(STRIDE_SAMPLES, 207_360);
    }

    #[test]
    fn window_covers_long_audio_exactly_once() {
        // 147 s, la duree de l'enregistrement qui a revele le bug.
        let n = 147 * SAMPLE_RATE + 1_234;
        let samples: Vec<f32> = (0..n).map(|i| 0.1 * ((i as f32) * 0.01).sin()).collect();
        let w = plan_windows(&samples);
        assert!(w.len() > 1);
        assert_eq!(w[0].start, 0);
        assert_eq!(w[0].own_from, 0);
        assert_eq!(w[w.len() - 1].end, n);
        for win in &w {
            assert!(win.end - win.start <= WINDOW_SAMPLES);
            assert!(win.start <= win.own_from && win.own_from < win.end);
            assert_eq!(win.start % FRAME_SAMPLES, 0);
        }
        // Chaque fenetre recouvre la precedente (pas de trou dans l'audio),
        // et les parties "propres" avancent.
        for pair in w.windows(2) {
            assert!(pair[1].own_from > pair[0].own_from);
            assert!(pair[1].start < pair[0].end);
            assert!(pair[1].own_from >= pair[0].own_from + FRAME_SAMPLES);
        }
    }

    #[test]
    fn start_lands_on_silence() {
        // Tout est bruyant sauf un trou de 0,4 s pres du debut prevu (12,96 s).
        let n = 30 * SAMPLE_RATE;
        let mut samples = vec![0.3f32; n];
        let hole_from = STRIDE_SAMPLES + 5 * FRAME_SAMPLES;
        for s in &mut samples[hole_from..hole_from + 6_400] {
            *s = 0.0;
        }
        let w = plan_windows(&samples);
        assert!(w.len() >= 2);
        assert!(
            (hole_from..hole_from + 6_400).contains(&w[1].own_from),
            "debut {} hors du silence",
            w[1].own_from
        );
    }

    #[test]
    fn planned_start_is_kept_without_silence() {
        let n = 30 * SAMPLE_RATE;
        let samples: Vec<f32> = (0..n).map(|i| 0.3 * ((i as f32) * 0.05).sin()).collect();
        let w = plan_windows(&samples);
        assert!(w.len() >= 2);
        // Pas de creux net : le debut prevu est garde.
        assert_eq!(w[1].own_from, STRIDE_SAMPLES);
    }

    #[test]
    fn short_last_window_is_extended_backwards() {
        // 20 s de bruit : la 2e fenetre commence vers 13 s et n'a que 7 s.
        let n = 20 * SAMPLE_RATE;
        let samples: Vec<f32> = (0..n).map(|i| 0.3 * ((i as f32) * 0.05).sin()).collect();
        let w = plan_windows(&samples);
        assert_eq!(w.len(), 2);
        let last = &w[1];
        assert_eq!(last.end, n);
        assert!(last.start < last.own_from, "fenetre non prolongee");
        // Fenetre pleine (a une trame pres), jamais remplie de zeros.
        assert!(WINDOW_SAMPLES - (last.end - last.start) < FRAME_SAMPLES);
    }

    #[test]
    fn window_after_end_of_speech_is_skipped() {
        // 10 s de parole puis 20 s de silence numerique : une seule fenetre.
        let n = 30 * SAMPLE_RATE;
        let mut samples = vec![0.0f32; n];
        for (i, s) in samples[..10 * SAMPLE_RATE].iter_mut().enumerate() {
            *s = 0.3 * ((i as f32) * 0.05).sin();
        }
        let w = plan_windows(&samples);
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].start, 0);
        assert!(w[0].end <= 10 * SAMPLE_RATE + FRAME_SAMPLES);
    }

    #[test]
    fn clean_overlap_merges_without_duplicates() {
        // Mots toutes les 0,4 s ; la gauche couvre 0-14 s, la droite 12-26 s.
        let all: Vec<String> = (0..65).map(|i| format!("w{i}")).collect();
        let refs: Vec<&str> = all.iter().map(String::as_str).collect();
        let full = words(&refs, 0.0, 0.4);
        let left: Vec<_> = full.iter().filter(|t| t.start < 14.0).cloned().collect();
        let right: Vec<_> = full.iter().filter(|t| t.start >= 12.0).cloned().collect();
        let merged = merge_windows(vec![left, right]);
        assert_eq!(tokens_to_text(&merged), tokens_to_text(&full));
    }

    #[test]
    fn no_match_cuts_at_midpoint() {
        // Deux fenetres qui ne partagent aucun mot : coupe a (fin gauche + debut droite) / 2.
        let left = words(&["a", "b", "c", "d", "e", "f"], 10.0, 0.4); // 10.0 .. 12.0, fin 12.08
        let right = words(&["u", "v", "w", "x", "y", "z"], 11.0, 0.4); // 11.0 .. 13.0
        let merged = merge_windows(vec![left, right]);
        // milieu = (12.08 + 11.0) / 2 = 11.54 : a, b, c (10.0 10.4 10.8 11.2 -> a..d avant),
        // gauche < 11.54 = a b c d ; droite >= 11.54 = w(11.8)? u 11.0, v 11.4, w 11.8.
        assert_eq!(tokens_to_text(&merged), "a b c d w x y z");
    }

    #[test]
    fn midpoint_cut_does_not_split_a_word() {
        let left = vec![
            tok(" un", 10.0),
            tok(" mot", 10.4),
            tok(" cou", 11.4),
            tok("pe", 11.9),
            tok(" fin", 12.2),
        ];
        // Aucun jeton commun : coupe a (12,28 + 11,0) / 2 = 11,64. La gauche
        // finit son mot "coupe", la droite saute le morceau orphelin "orph".
        let right = vec![
            tok(" zz", 11.0),
            tok(" yy", 11.3),
            tok("orph", 11.7),
            tok(" ab", 12.0),
            tok(" cd", 12.4),
        ];
        let merged = merge_windows(vec![left, right]);
        assert_eq!(tokens_to_text(&merged), "un mot coupe ab cd");
    }

    #[test]
    fn splice_inside_a_word_takes_the_whole_word_from_the_right() {
        // Les deux fenetres entendent "bonjour tout le monde" mais ne decoupent
        // pas "monde" pareil : la gauche " m" + "onde", la droite " m" + "on" + "de".
        let left = vec![
            tok(" bonjour", 10.0),
            tok(" tout", 10.4),
            tok(" le", 10.8),
            tok(" m", 11.2),
            tok("onde", 11.28),
        ];
        let right = vec![
            tok(" tout", 10.4),
            tok(" le", 10.8),
            tok(" m", 11.2),
            tok("on", 11.28),
            tok("de", 11.36),
            tok(" aujourd", 11.6),
            tok("'hui", 11.7),
        ];
        let merged = merge_windows(vec![left, right]);
        assert_eq!(tokens_to_text(&merged), "bonjour tout le monde aujourd'hui");
        // Le mot n'apparait qu'une fois, avec le decoupage de la droite.
        let pieces: Vec<&str> = merged.iter().map(|t| t.text.as_str()).collect();
        assert!(pieces.contains(&"on") && !pieces.contains(&"onde"));
    }

    #[test]
    fn splice_after_matches_never_glues_a_continuation() {
        // La derniere paire commune est " ci" ; la droite continue par "ne" (suite du mot).
        let left = vec![
            tok(" voici", 10.0),
            tok(" le", 10.4),
            tok(" ci", 10.8),
            tok("ne", 10.88),
            tok("ma", 10.96),
        ];
        let right = vec![
            tok(" voici", 10.0),
            tok(" le", 10.4),
            tok(" ci", 10.8),
            tok("nema", 10.88),
            tok(" est", 11.4),
        ];
        let merged = merge_windows(vec![left, right]);
        let text = tokens_to_text(&merged);
        assert_eq!(text, "voici le cinema est");
    }

    #[test]
    fn case_only_duplicate_is_removed_at_a_seam() {
        let left = vec![
            tok(" We", 10.0),
            tok(" have", 10.4),
            tok(" Have", 10.8),
            tok(" a", 11.2),
            tok(" plan", 11.6),
        ];
        // Pas de recouvrement exploitable : concatenation, le doublon reste a la couture.
        let right_after = words(&["today"], 12.0, 0.4);
        let merged = merge_windows(vec![left, right_after]);
        assert_eq!(tokens_to_text(&merged), "We have a plan today");
    }

    #[test]
    fn case_duplicate_far_from_seam_is_kept() {
        let tokens = words(&["have", "Have", "a", "plan"], 1.0, 0.4);
        let out = collapse_seam_duplicates(tokens.clone(), &[(100.0, 102.0)]);
        assert_eq!(tokens_to_text(&out), tokens_to_text(&tokens));
    }

    #[test]
    fn timestamps_never_go_backwards() {
        let mut t = vec![tok(" a", 1.0), tok(" b", 0.9), tok(" c", 1.5)];
        enforce_monotonic(&mut t);
        assert_eq!(
            t.iter().map(|x| x.start).collect::<Vec<_>>(),
            vec![1.0, 1.0, 1.5]
        );
        assert_eq!(tokens_to_text(&t), "a b c");
    }

    #[test]
    fn subword_tokens_join_like_a_single_pass() {
        let t = vec![
            tok(" l'en", 1.0),
            tok("ti", 1.1),
            tok("erete", 1.2),
            tok(",", 1.3),
            tok(" du", 1.5),
        ];
        assert_eq!(tokens_to_text(&t), "l'entierete, du");
    }

    #[test]
    fn last_window_borrowed_tokens_are_dropped() {
        // Audio de 20 s sans silence : 2 fenetres, la 2e a de l'audio emprunte.
        let n = 20 * SAMPLE_RATE;
        let samples: Vec<f32> = (0..n).map(|i| 0.3 * ((i as f32) * 0.05).sin()).collect();
        // Un faux decodeur : un mot par seconde de la tranche, nomme par son
        // temps absolu (on retrouve la tranche grace a sa longueur et a l'audio).
        let plan = plan_windows(&samples);
        assert_eq!(plan.len(), 2);
        let mut calls = 0usize;
        let (text, count) = transcribe_in_windows(&samples, |slice| {
            let w = &plan[calls];
            calls += 1;
            assert_eq!(slice.len(), w.end - w.start);
            let offset = w.start as f32 / SAMPLE_RATE as f32;
            let secs = slice.len() as f32 / SAMPLE_RATE as f32;
            // Mots absolus aux secondes entieres 1..=19, vus de la fenetre.
            Ok((1..20)
                .filter(|s| (*s as f32) >= offset && (*s as f32) < offset + secs - 0.5)
                .map(|s| tok(&format!(" m{s}"), s as f32 - offset))
                .collect())
        })
        .unwrap();
        assert_eq!(count, 2);
        let expected: Vec<String> = (1..20).map(|s| format!("m{s}")).collect();
        assert_eq!(text, expected.join(" "));
    }
}
