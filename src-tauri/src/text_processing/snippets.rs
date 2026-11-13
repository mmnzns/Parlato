// Parlato : extraits. Quand le texte dicte contient la phrase d'un extrait
// (son declencheur), la phrase est remplacee par le texte de l'extrait.
//
// Regles (decision, voir CHANGELOG) :
//   - Le declencheur se reconnait sur des mots entiers, sans tenir compte de
//     la casse, des accents ni de la ponctuation autour ; entre ses mots, que
//     des espaces. Les declencheurs les plus longs passent d'abord.
//   - Si la dictee entiere est le declencheur ("My address."), le resultat est
//     le texte de l'extrait seul, sans la ponctuation du modele, et le
//     nettoyage IA est saute (rien a nettoyer, le texte est a l'identique).
//   - Sinon la phrase est remplacee dans le texte ; ce qui l'entoure (virgule,
//     point) reste.
//   - Le texte d'un extrait n'est jamais retraite : l'IA de nettoyage ne le
//     voit pas. Pendant le nettoyage, la place de l'extrait est tenue par un
//     marqueur `[[snippet-N]]` ; si l'IA en perd un, son resultat est jete et
//     le texte non nettoye est colle (`restore` rend `None`).
//   - Les extraits passent apres les commandes vocales, la ponctuation
//     parlee et les mots du dictionnaire, donc rien ne les modifie.

use super::phrase;
use crate::db::snippet::Snippet;

const OPEN: &str = "[[snippet-";
const CLOSE: &str = "]]";

/// Un extrait insere : son marqueur et son texte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slot {
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Applied {
    /// Aucun declencheur dans le texte.
    Unchanged,
    /// La dictee entiere etait un declencheur : le texte de l'extrait.
    Whole(String),
    /// Des declencheurs dans le texte, remplaces par des marqueurs ; `slots`
    /// donne le texte de chacun (marqueur `[[snippet-N]]`, N des 1).
    Marked { text: String, slots: Vec<Slot> },
}

fn marker(n: usize) -> String {
    format!("{OPEN}{n}{CLOSE}")
}

/// Cherche les declencheurs de `snippets` (les actifs seulement) dans `text`.
pub fn apply(text: &str, snippets: &[Snippet]) -> Applied {
    let mut rules: Vec<(Vec<String>, &Snippet)> = snippets
        .iter()
        .filter(|s| s.is_enabled)
        .map(|s| (phrase::keys(&s.trigger_text), s))
        .filter(|(keys, _)| !keys.is_empty())
        .collect();
    if rules.is_empty() {
        return Applied::Unchanged;
    }
    rules.sort_by_key(|(keys, _)| std::cmp::Reverse(keys.len()));

    let words = phrase::tokenize(text);
    // The whole dictation is one trigger.
    let text_keys: Vec<&str> = words.iter().map(|w| w.key.as_str()).collect();
    if let Some((_, s)) = rules
        .iter()
        .find(|(keys, _)| keys.iter().map(String::as_str).eq(text_keys.iter().copied()))
    {
        return Applied::Whole(s.content.clone());
    }

    let mut out = String::with_capacity(text.len());
    let mut slots: Vec<Slot> = Vec::new();
    let mut cursor = 0;
    let mut i = 0;
    while i < words.len() {
        let hit = rules
            .iter()
            .find_map(|(keys, s)| phrase::matches_at(text, &words, i, keys, false).map(|n| (n, *s)));
        match hit {
            Some((n, snippet)) => {
                out.push_str(&text[cursor..words[i].start]);
                slots.push(Slot { content: snippet.content.clone() });
                out.push_str(&marker(slots.len()));
                cursor = words[i + n - 1].end;
                i += n;
            }
            None => i += 1,
        }
    }
    if slots.is_empty() {
        return Applied::Unchanged;
    }
    out.push_str(&text[cursor..]);
    Applied::Marked { text: out, slots }
}

/// Remplace chaque marqueur de `text` par le texte de son extrait, en une
/// seule passe (le texte d'un extrait n'est jamais relu). Rend aussi les
/// numeros des marqueurs trouves.
fn fill(text: &str, slots: &[Slot]) -> (String, Vec<usize>) {
    let mut out = String::with_capacity(text.len());
    let mut used = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find(OPEN) {
        let after = &rest[at + OPEN.len()..];
        let parsed = after.find(CLOSE).and_then(|end| {
            after[..end].parse::<usize>().ok().filter(|n| (1..=slots.len()).contains(n)).map(|n| (n, end))
        });
        match parsed {
            Some((n, end)) => {
                out.push_str(&rest[..at]);
                out.push_str(&slots[n - 1].content);
                used.push(n);
                rest = &after[end + CLOSE.len()..];
            }
            None => {
                out.push_str(&rest[..at + OPEN.len()]);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    (out, used)
}

/// Le texte avec les marqueurs remplaces (sans verifier qu'ils y sont tous).
pub fn fill_all(text: &str, slots: &[Slot]) -> String {
    fill(text, slots).0
}

/// Le texte rendu par le nettoyage IA, marqueurs remplaces ; `None` si l'IA
/// en a perdu ou change un.
pub fn restore(text: &str, slots: &[Slot]) -> Option<String> {
    let (out, used) = fill(text, slots);
    (1..=slots.len()).all(|n| used.contains(&n)).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn snip(trigger: &str, content: &str) -> Snippet {
        Snippet {
            id: trigger.into(),
            trigger_text: trigger.into(),
            content: content.into(),
            date_added: Utc::now(),
            is_enabled: true,
        }
    }

    fn marked(a: Applied) -> (String, Vec<String>) {
        match a {
            Applied::Marked { text, slots } => (text, slots.into_iter().map(|s| s.content).collect()),
            other => panic!("not marked: {other:?}"),
        }
    }

    #[test]
    fn the_whole_dictation_ignores_case_and_punctuation() {
        let s = [snip("my address", "1 Main St\nSpringfield")];
        for said in ["my address", "My address.", "  MY ADDRESS!  ", "My, address?"] {
            assert_eq!(apply(said, &s), Applied::Whole("1 Main St\nSpringfield".into()), "{said}");
        }
    }

    #[test]
    fn a_trigger_inside_a_sentence_is_replaced_and_the_rest_kept() {
        let s = [snip("my email", "me@example.com")];
        let (text, slots) = marked(apply("Write to my email, thanks.", &s));
        assert_eq!(text, "Write to [[snippet-1]], thanks.");
        assert_eq!(slots, ["me@example.com"]);
        assert_eq!(fill_all(&text, &[Slot { content: slots[0].clone() }]), "Write to me@example.com, thanks.");
    }

    #[test]
    fn several_triggers_and_repeats() {
        let s = [snip("sig", "Best,\nMe"), snip("my email", "me@x.com")];
        let (text, slots) = marked(apply("Mail my email or sig then sig", &s));
        assert_eq!(text, "Mail [[snippet-1]] or [[snippet-2]] then [[snippet-3]]");
        assert_eq!(slots, ["me@x.com", "Best,\nMe", "Best,\nMe"]);
    }

    #[test]
    fn the_longest_trigger_wins() {
        let s = [snip("my address", "short"), snip("my address at work", "long")];
        let (text, slots) = marked(apply("send it to my address at work please", &s));
        assert_eq!(text, "send it to [[snippet-1]] please");
        assert_eq!(slots, ["long"]);
        assert_eq!(apply("my address at work", &s), Applied::Whole("long".into()));
    }

    #[test]
    fn whole_words_only() {
        let s = [snip("sig", "Me")];
        assert_eq!(apply("design signature", &s), Applied::Unchanged);
        assert_eq!(apply("a sigma", &s), Applied::Unchanged);
    }

    #[test]
    fn spanish_and_french_triggers_ignore_accents() {
        let s = [snip("mi dirección", "Calle 1"), snip("ma signature", "Cordialement")];
        assert_eq!(apply("Mi direccion.", &s), Applied::Whole("Calle 1".into()));
        assert_eq!(apply("MA SIGNATURE", &s), Applied::Whole("Cordialement".into()));
    }

    #[test]
    fn disabled_snippets_and_empty_lists_do_nothing() {
        let mut off = snip("my email", "x");
        off.is_enabled = false;
        assert_eq!(apply("my email", &[off]), Applied::Unchanged);
        assert_eq!(apply("my email", &[]), Applied::Unchanged);
        assert_eq!(apply("", &[snip("a", "b")]), Applied::Unchanged);
    }

    #[test]
    fn a_trigger_without_words_never_matches() {
        assert_eq!(apply("hello", &[snip("!!!", "x")]), Applied::Unchanged);
    }

    #[test]
    fn a_gap_with_punctuation_is_not_the_trigger_within_a_sentence() {
        let s = [snip("my email", "x")];
        assert_eq!(apply("my, email is here", &s), Applied::Unchanged);
    }

    #[test]
    fn snippet_text_is_inserted_as_is() {
        // "$1", brackets and trigger-like text are not interpreted.
        let s = [snip("price", "costs $1 and [[snippet-2]] and my email"), snip("my email", "x")];
        let (text, slots) = marked(apply("the price today", &s));
        let slots: Vec<Slot> = slots.into_iter().map(|content| Slot { content }).collect();
        assert_eq!(fill_all(&text, &slots), "the costs $1 and [[snippet-2]] and my email today");
    }

    #[test]
    fn restore_needs_every_marker_back() {
        let slots = vec![Slot { content: "A".into() }, Slot { content: "B".into() }];
        assert_eq!(restore("x [[snippet-1]] y [[snippet-2]]", &slots).as_deref(), Some("x A y B"));
        // Order may change.
        assert_eq!(restore("[[snippet-2]] then [[snippet-1]]", &slots).as_deref(), Some("B then A"));
        // The AI dropped one, or changed its brackets.
        assert_eq!(restore("x [[snippet-1]] y", &slots), None);
        assert_eq!(restore("x [snippet-1] y [[snippet-2]]", &slots), None);
        // An unknown number is left as it is and does not count.
        assert_eq!(restore("[[snippet-9]] [[snippet-1]]", &[Slot { content: "A".into() }]).as_deref(), Some("[[snippet-9]] A"));
    }
}
