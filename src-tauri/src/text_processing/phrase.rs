// Parlato : reconnaissance d'une phrase (suite de mots entiers) dans un texte.
//
// Partagee par les commandes vocales (`spoken.rs`) et les extraits
// (`snippets.rs`). Un mot est une suite de lettres, de chiffres et
// d'apostrophes ; la casse, les accents et la ponctuation autour ne comptent
// pas. Une phrase ne matche que des mots entiers : "new line" ne matche pas
// dans "renew lines" ni "newline" (celui-la est une phrase a part).

/// Un mot du texte : ses octets dans le texte et sa cle de comparaison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word {
    pub start: usize,
    pub end: usize,
    pub key: String,
}

fn is_apostrophe(c: char) -> bool {
    c == '\'' || c == '\u{2019}'
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || is_apostrophe(c)
}

/// Cle de comparaison d'un mot : minuscules, sans accents, apostrophe droite.
pub fn fold(word: &str) -> String {
    let mut out = String::with_capacity(word.len());
    for lower in word.chars().flat_map(char::to_lowercase) {
        out.push(match lower {
            'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' => 'a',
            'ç' => 'c',
            'è' | 'é' | 'ê' | 'ë' => 'e',
            'ì' | 'í' | 'î' | 'ï' => 'i',
            'ñ' => 'n',
            'ò' | 'ó' | 'ô' | 'ö' | 'õ' => 'o',
            'ù' | 'ú' | 'û' | 'ü' => 'u',
            'ý' | 'ÿ' => 'y',
            '\u{2019}' => '\'',
            c => c,
        });
    }
    out
}

/// Les mots de `text`, dans l'ordre. Les apostrophes en bord de mot (guillemets
/// simples) ne font pas partie du mot.
pub fn tokenize(text: &str) -> Vec<Word> {
    let mut words = Vec::new();
    let mut start: Option<usize> = None;
    let close = |from: usize, to: usize, words: &mut Vec<Word>| {
        let raw = &text[from..to];
        let trimmed = raw.trim_matches(is_apostrophe);
        if trimmed.is_empty() {
            return;
        }
        let lead = raw.len() - raw.trim_start_matches(is_apostrophe).len();
        words.push(Word {
            start: from + lead,
            end: from + lead + trimmed.len(),
            key: fold(trimmed),
        });
    };
    for (i, c) in text.char_indices() {
        if is_word_char(c) {
            if start.is_none() {
                start = Some(i);
            }
        } else if let Some(s) = start.take() {
            close(s, i, &mut words);
        }
    }
    if let Some(s) = start {
        close(s, text.len(), &mut words);
    }
    words
}

/// Les cles des mots d'une phrase ("Nueva línea" -> ["nueva", "linea"]).
pub fn keys(phrase: &str) -> Vec<String> {
    tokenize(phrase).into_iter().map(|w| w.key).collect()
}

/// Combien de mots de `words`, a partir de `at`, forment `phrase` (sinon
/// `None`) ? Entre deux mots de la phrase, il ne doit y avoir que des
/// espaces ; avec `lenient`, aussi des virgules et des traits d'union (les
/// modeles de parole ecrivent "new, line" ou "point-virgule").
pub fn matches_at(text: &str, words: &[Word], at: usize, phrase: &[String], lenient: bool) -> Option<usize> {
    if phrase.is_empty() || at + phrase.len() > words.len() {
        return None;
    }
    for (j, key) in phrase.iter().enumerate() {
        if &words[at + j].key != key {
            return None;
        }
        if j > 0 {
            let gap = &text[words[at + j - 1].end..words[at + j].start];
            let ok = gap
                .chars()
                .all(|c| c.is_whitespace() || (lenient && (c == ',' || c == '-')));
            if !ok {
                return None;
            }
        }
    }
    Some(phrase.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fold_ignores_case_and_accents() {
        assert_eq!(fold("Nueva"), "nueva");
        assert_eq!(fold("línea"), "linea");
        assert_eq!(fold("ÇA"), "ca");
        assert_eq!(fold("d\u{2019}interrogation"), "d'interrogation");
        assert_eq!(fold("Ñandú"), "nandu");
    }

    #[test]
    fn tokenize_splits_on_everything_but_letters_digits_and_apostrophes() {
        let t = "Hello, world! d'accord 3.5";
        let w = tokenize(t);
        let keys: Vec<&str> = w.iter().map(|w| w.key.as_str()).collect();
        assert_eq!(keys, ["hello", "world", "d'accord", "3", "5"]);
        assert_eq!(&t[w[1].start..w[1].end], "world");
    }

    #[test]
    fn quote_marks_around_a_word_are_not_part_of_it() {
        let t = "say 'hello' now";
        let w = tokenize(t);
        assert_eq!(&t[w[1].start..w[1].end], "hello");
        assert!(tokenize("'' ' ").is_empty());
    }

    #[test]
    fn matches_whole_words_only() {
        let t = "renew lines and the new line";
        let w = tokenize(t);
        let p = keys("new line");
        let hits: Vec<usize> = (0..w.len()).filter(|&i| matches_at(t, &w, i, &p, false).is_some()).collect();
        assert_eq!(hits, [4]);
    }

    #[test]
    fn a_gap_may_only_hold_spaces_unless_lenient() {
        let t = "new, line";
        let w = tokenize(t);
        let p = keys("new line");
        assert_eq!(matches_at(t, &w, 0, &p, false), None);
        assert_eq!(matches_at(t, &w, 0, &p, true), Some(2));
        let t = "new. line";
        let w = tokenize(t);
        assert_eq!(matches_at(t, &w, 0, &p, true), None);
        let t = "point-virgule";
        let w = tokenize(t);
        assert_eq!(matches_at(t, &w, 0, &keys("point virgule"), true), Some(2));
    }

    #[test]
    fn a_phrase_longer_than_the_rest_does_not_match() {
        let t = "new";
        let w = tokenize(t);
        assert_eq!(matches_at(t, &w, 0, &keys("new line"), false), None);
        assert_eq!(matches_at(t, &w, 0, &[], false), None);
    }
}
