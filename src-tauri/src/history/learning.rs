// Parlato : apprentissage du dictionnaire. Quand la personne corrige le
// texte d'une dictee dans l'historique, on compare l'ancien et le nouveau
// texte mot a mot et on propose les mots changes (noms propres, termes
// techniques) pour le dictionnaire. Rien n'est ajoute sans un clic : ce
// module ne fait que calculer la liste des suggestions.
//
// Regles :
//  - comparaison au niveau des mots (plus longue sous-suite commune), sans
//    tenir compte de la casse ni de la ponctuation autour des mots ;
//  - un changement de casse seul est ignore, sauf si le mot corrige prend
//    une majuscule au milieu d'une phrase (nom propre : "kubernetes" ->
//    "Kubernetes") ou une casse mixte ("github" -> "GitHub") ;
//  - les mots tres courants (en, es, fr), les nombres et les mots deja dans
//    le dictionnaire sont ignores ;
//  - une reecriture complete n'est pas une correction : aucune suggestion.

/// Nombre maximal de suggestions montrees d'un coup.
const MAX_SUGGESTIONS: usize = 5;
/// Au-dela de ce nombre de mots changes (ou de cette part du texte), on
/// considere que le texte a ete reecrit.
const MAX_CHANGED_WORDS: usize = 8;

#[derive(Debug, Clone)]
struct Token {
    /// Le mot sans ponctuation en tete ni en queue.
    text: String,
    /// Le mot en minuscules (cle de comparaison).
    key: String,
    /// Premier mot du texte ou d'une phrase.
    sentence_start: bool,
}

fn tokenize(text: &str) -> Vec<Token> {
    let mut out = Vec::new();
    for line in text.lines() {
        // A new line starts a new sentence.
        let mut sentence_start = true;
        for raw in line.split_whitespace() {
            let trimmed = raw.trim_matches(|c: char| !c.is_alphanumeric());
            if trimmed.is_empty() {
                continue;
            }
            out.push(Token {
                text: trimmed.to_string(),
                key: trimmed.to_lowercase(),
                sentence_start,
            });
            sentence_start = raw
                .trim_end_matches(['"', '\'', ')', '\u{201d}', '\u{bb}'])
                .ends_with(['.', '!', '?', '\u{2026}']);
        }
    }
    out
}

fn is_capitalized(s: &str) -> bool {
    s.chars().next().is_some_and(char::is_uppercase)
}

/// "GitHub", "iPhone", "OpenAI": an uppercase letter after the first char
/// together with a lowercase one somewhere (all caps like "NASA" count too
/// when more than one letter).
fn has_inner_capital(s: &str) -> bool {
    s.chars().skip(1).any(char::is_uppercase)
}

fn is_all_lowercase(s: &str) -> bool {
    !s.chars().any(char::is_uppercase)
}

fn plausible(word: &Token) -> bool {
    let letters = word.text.chars().filter(|c| c.is_alphabetic()).count();
    // "OK" and the like are not worth a Dictionary entry.
    let short_caps = letters < 3 && word.text.chars().all(|c| !c.is_lowercase());
    letters >= 2 && !short_caps && !is_common_word(&word.key)
}

/// Words too common to be worth teaching the Dictionary (English, Spanish,
/// French). Kept small on purpose: the point is to skip the glue words that a
/// typo fix would otherwise surface.
fn is_common_word(key: &str) -> bool {
    const COMMON: &[&str] = &[
        // English
        "the", "and", "for", "are", "but", "not", "you", "all", "can", "had", "her", "was", "one", "our",
        "out", "has", "have", "him", "his", "how", "its", "may", "new", "now", "old", "see", "two", "who",
        "did", "get", "she", "use", "that", "this", "with", "from", "they", "will", "would", "there",
        "their", "what", "about", "which", "when", "make", "like", "time", "just", "know", "take", "into",
        "year", "your", "some", "could", "them", "than", "then", "also", "been", "were", "said", "each",
        "does", "done", "more", "most", "much", "very", "over", "here", "where", "should", "these", "those",
        "other", "going", "want", "need", "think", "because", "really", "thing", "things", "good", "well",
        "back", "only", "come", "look", "work", "first", "after", "before", "again", "right", "still",
        // Spanish
        "que", "los", "las", "una", "uno", "por", "con", "para", "del", "como", "pero", "sus", "muy",
        "más", "mas", "sin", "sobre", "también", "tambien", "esta", "este", "estos", "estas", "eso",
        "esto", "todo", "todos", "cuando", "donde", "porque", "entre", "hasta", "desde", "hay", "ser",
        "son", "fue", "han", "tiene", "tengo", "hacer", "puede", "ahora", "aquí", "aqui", "así", "asi",
        "bien", "cada", "otro", "otra", "algo", "nada", "mismo", "dónde", "qué", "cómo", "está", "están",
        // French
        "les", "des", "une", "est", "pas", "pour", "dans", "qui", "sur", "avec", "mais", "plus", "tout",
        "tous", "nous", "vous", "ils", "elle", "elles", "ses", "mes", "tes", "cette", "cet", "ces",
        "être", "avoir", "fait", "faire", "comme", "aussi", "très", "tres", "bien", "sans", "sous",
        "entre", "chez", "alors", "donc", "puis", "encore", "même", "meme", "autre", "ainsi", "leur",
        "leurs", "quand", "parce", "peut", "sont", "été", "ete", "était", "etait", "ça", "voir",
    ];
    COMMON.contains(&key)
}

/// Words worth suggesting for the Dictionary after `old` became `new`.
/// `known` are the words already in the Dictionary (any case).
pub fn suggest_words(old: &str, new: &str, known: &[String]) -> Vec<String> {
    let old_t = tokenize(old);
    let new_t = tokenize(new);

    // Trim the common prefix and suffix first: edits are local, and the
    // quadratic table below only runs on the middle.
    let mut start = 0;
    while start < old_t.len() && start < new_t.len() && old_t[start].key == new_t[start].key {
        start += 1;
    }
    let mut end_old = old_t.len();
    let mut end_new = new_t.len();
    while end_old > start && end_new > start && old_t[end_old - 1].key == new_t[end_new - 1].key {
        end_old -= 1;
        end_new -= 1;
    }
    let mid_old = &old_t[start..end_old];
    let mid_new = &new_t[start..end_new];
    if mid_old.len().saturating_mul(mid_new.len()) > 4_000_000 {
        return Vec::new();
    }

    let mut candidates: Vec<&Token> = Vec::new();
    // Case-only changes inside the untouched prefix, suffix and matches.
    for (o, n) in old_t[..start].iter().zip(&new_t[..start]) {
        if case_change_worth_teaching(o, n) {
            candidates.push(n);
        }
    }
    for (o, n) in old_t[end_old..].iter().zip(&new_t[end_new..]) {
        if case_change_worth_teaching(o, n) {
            candidates.push(n);
        }
    }

    // Longest common subsequence on the middle.
    let (n, m) = (mid_old.len(), mid_new.len());
    let mut table = vec![0u32; (n + 1) * (m + 1)];
    let at = |i: usize, j: usize| i * (m + 1) + j;
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            table[at(i, j)] = if mid_old[i].key == mid_new[j].key {
                table[at(i + 1, j + 1)] + 1
            } else {
                table[at(i + 1, j)].max(table[at(i, j + 1)])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut changed = 0usize;
    while j < m {
        if i < n && mid_old[i].key == mid_new[j].key {
            if case_change_worth_teaching(&mid_old[i], &mid_new[j]) {
                candidates.push(&mid_new[j]);
            }
            i += 1;
            j += 1;
        } else if i < n && table[at(i + 1, j)] >= table[at(i, j + 1)] {
            i += 1; // word removed
        } else {
            changed += 1;
            // Inserted or replaced word: a candidate unless the same word
            // already stood in the old text.
            if !old_t.iter().any(|o| o.key == mid_new[j].key) {
                candidates.push(&mid_new[j]);
            }
            j += 1;
        }
    }

    // A rewrite is not a correction.
    let total = old_t.len().max(1);
    if changed > MAX_CHANGED_WORDS || changed * 2 > total.max(4) {
        return Vec::new();
    }

    let known: Vec<String> = known.iter().map(|k| k.trim().to_lowercase()).collect();
    let mut out: Vec<String> = Vec::new();
    for c in candidates {
        if !plausible(c)
            || c.text.chars().all(|ch| ch.is_ascii_digit())
            || known.contains(&c.key)
            || out.iter().any(|w| w.to_lowercase() == c.key)
        {
            continue;
        }
        out.push(c.text.clone());
        if out.len() == MAX_SUGGESTIONS {
            break;
        }
    }
    out
}

/// Same word in both texts but the case differs: worth teaching only when
/// the new spelling is a proper noun (capital in mid-sentence, old one all
/// lowercase) or has an inner capital (GitHub), and is not a common word.
fn case_change_worth_teaching(old: &Token, new: &Token) -> bool {
    if old.text == new.text {
        return false;
    }
    let proper = is_capitalized(&new.text) && !new.sentence_start && is_all_lowercase(&old.text);
    let mixed = has_inner_capital(&new.text) && new.text != old.text;
    proper || mixed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(old: &str, new: &str) -> Vec<String> {
        suggest_words(old, new, &[])
    }

    #[test]
    fn replaced_words_are_suggested() {
        assert_eq!(
            s("we deploy on cooper nettys today", "we deploy on Kubernetes today"),
            vec!["Kubernetes"]
        );
    }

    #[test]
    fn several_names_in_one_edit() {
        assert_eq!(
            s(
                "send it to acne and cooper nettys team",
                "send it to Acme and Kubernetes team"
            ),
            vec!["Acme", "Kubernetes"]
        );
    }

    #[test]
    fn punctuation_only_changes_are_ignored() {
        assert!(s("hello world how are you", "Hello, world. How are you?").is_empty());
        assert!(s("one two three", "one, two, three!").is_empty());
    }

    #[test]
    fn sentence_start_capital_is_ignored() {
        assert!(s("hello there. dinner is ready", "Hello there. Dinner is ready").is_empty());
    }

    #[test]
    fn proper_noun_capital_mid_sentence_is_suggested() {
        assert_eq!(s("i talked to maria yesterday", "I talked to Maria yesterday"), vec!["Maria"]);
    }

    #[test]
    fn inner_capitals_are_suggested() {
        assert_eq!(s("push it to github now", "push it to GitHub now"), vec!["GitHub"]);
    }

    #[test]
    fn capital_after_a_period_or_new_line_is_a_sentence_start() {
        assert!(s("it works. fine then", "It works. Fine then").is_empty());
        assert!(s("first line\nsecond line", "First line\nSecond line").is_empty());
    }

    #[test]
    fn lowercasing_is_never_suggested() {
        assert!(s("call Maria now", "call maria now").is_empty());
    }

    #[test]
    fn common_words_are_skipped() {
        assert!(s("their going home", "they are going home").is_empty());
        assert!(s("quiero el pan", "quiero los pan").is_empty());
        assert!(s("je veux le pain", "je veux les pain").is_empty());
    }

    #[test]
    fn numbers_and_one_letter_words_are_skipped() {
        assert!(s("we have ten", "we have 10").is_empty());
        assert!(s("send it to x", "send it to y").is_empty());
    }

    #[test]
    fn words_already_in_the_dictionary_are_skipped() {
        let known = vec!["kubernetes".to_string()];
        assert!(suggest_words("on cooper nettys", "on Kubernetes", &known).is_empty());
    }

    #[test]
    fn a_rewrite_gives_no_suggestions() {
        assert!(s(
            "this is the old text that was dictated yesterday",
            "completely different sentence about Kubernetes clusters and Acme budgets for quarterly planning"
        )
        .is_empty());
    }

    #[test]
    fn identical_text_gives_nothing() {
        assert!(s("same text here", "same text here").is_empty());
        assert!(s("", "").is_empty());
    }

    #[test]
    fn added_name_at_the_end_is_suggested_once() {
        assert_eq!(s("thanks from the", "thanks from the Zendesk Zendesk"), vec!["Zendesk"]);
    }

    #[test]
    fn accents_and_apostrophes_stay_in_the_word() {
        assert_eq!(
            s("rendez-vous avec jose demain", "rendez-vous avec José demain"),
            vec!["José"]
        );
        assert_eq!(s("it is on o'brien street", "it is on O'Brien street"), vec!["O'Brien"]);
    }

    #[test]
    fn suggestions_are_capped() {
        let keep = vec!["keep"; 20].join(" ");
        let old = format!("{keep} b c d e f g h");
        let new = format!("{keep} Alpha Bravo Charlie Delta Echo Foxtrot Golf");
        // 7 changes is under the rewrite limit but only 5 are offered.
        assert_eq!(suggest_words(&old, &new, &[]).len(), 5);
    }
}
