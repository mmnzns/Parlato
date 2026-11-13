// Parlato : commandes vocales et ponctuation parlee, appliquees au texte
// transcrit (avant le nettoyage IA et le collage).
//
// Deux groupes, deux reglages :
//   - Commands    ("Spoken commands", actif par defaut) : "new line" -> \n,
//     "new paragraph" -> \n\n, "scratch that" -> efface la derniere phrase.
//   - Punctuation ("Spoken punctuation", inactif par defaut) : "comma",
//     "period", "question mark", "open quote"... -> les symboles.
//
// Fonctions pures, sans etat. La phrase est reconnue sans tenir compte de la
// casse, des accents ni de la ponctuation que les modeles de parole ajoutent
// autour ("New line." ou ", new line,"), sur des mots entiers seulement.
//
// Langues (decision, voir docs/ et CHANGELOG) :
//   - Commandes : la langue de la dictee, plus toujours l'anglais ; langue
//     automatique : les trois.
//   - Ponctuation : la langue de la dictee seulement (les mots anglais
//     "period", "colon" existent dans les autres langues) ; langue
//     automatique : les trois, sans les mots trop courants ("punto", "coma",
//     "point" seuls).
//   - Une autre langue (allemand...) : commandes anglaises, pas de
//     ponctuation.
//
// Mise en forme (simple, la meme pour les trois langues, sauf le francais) :
//   - Un saut de ligne retire les espaces qui le precedent (la virgule reste :
//     "Dear Ann, new line") et la ponctuation que le modele a mise apres la
//     phrase ("New line.").
//   - Un symbole se colle au mot precedent et est suivi d'une espace.
//     Francais : une espace normale (pas insecable) avant ? ! : ; et
//     guillemets francais « ... » avec espaces.
//   - Apres un point, un point d'exclamation ou d'interrogation parles, ou
//     "new paragraph", la lettre suivante prend une majuscule. Pas apres
//     "new line".
//   - "scratch that" retire tout ce qui suit la derniere fin de phrase
//     (. ! ? ou un saut de ligne) avant lui ; si la phrase se terminait juste
//     avant lui ("Meet at five. Scratch that."), c'est celle-la qui part. Sans
//     rien avant : il ne fait rien.
//   - Limite connue : dans une phrase normale, "a new line of products" est lu
//     comme la commande. Le reglage se coupe.

use std::sync::OnceLock;

use super::phrase;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Commands,
    Punctuation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lang {
    En,
    Es,
    Fr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// Se colle au mot precedent (, . ? ...).
    Attach,
    /// Ouvre : espace avant, collee au mot suivant.
    Open,
    /// Ferme : collee au mot precedent.
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Symbol {
    text: &'static str,
    kind: Kind,
    /// Francais : espace avant ? ! : ; et ».
    space_before: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Line,
    Paragraph,
    Scratch,
    Symbol(Symbol),
}

struct Entry {
    lang: Lang,
    group: Group,
    /// Mots trop courants pour la langue automatique ("punto", "point").
    explicit_only: bool,
    keys: Vec<String>,
    action: Action,
}

const fn attach(text: &'static str) -> Action {
    Action::Symbol(Symbol { text, kind: Kind::Attach, space_before: false })
}
const fn attach_fr(text: &'static str) -> Action {
    Action::Symbol(Symbol { text, kind: Kind::Attach, space_before: true })
}
const fn open(text: &'static str) -> Action {
    Action::Symbol(Symbol { text, kind: Kind::Open, space_before: false })
}
const fn close(text: &'static str) -> Action {
    Action::Symbol(Symbol { text, kind: Kind::Close, space_before: false })
}
const fn close_fr(text: &'static str) -> Action {
    Action::Symbol(Symbol { text, kind: Kind::Close, space_before: true })
}

/// (langue, groupe, mots trop courants, phrase, action).
type Row = (Lang, Group, bool, &'static str, Action);

const fn cmd(lang: Lang, phrase: &'static str, action: Action) -> Row {
    (lang, Group::Commands, false, phrase, action)
}
const fn punct(lang: Lang, phrase: &'static str, action: Action) -> Row {
    (lang, Group::Punctuation, false, phrase, action)
}
const fn punct_bare(lang: Lang, phrase: &'static str, action: Action) -> Row {
    (lang, Group::Punctuation, true, phrase, action)
}

const ROWS: &[Row] = &[
    // English
    cmd(Lang::En, "new line", Action::Line),
    cmd(Lang::En, "newline", Action::Line),
    cmd(Lang::En, "new paragraph", Action::Paragraph),
    cmd(Lang::En, "scratch that", Action::Scratch),
    punct(Lang::En, "comma", attach(",")),
    punct(Lang::En, "period", attach(".")),
    punct(Lang::En, "full stop", attach(".")),
    punct(Lang::En, "question mark", attach("?")),
    punct(Lang::En, "exclamation mark", attach("!")),
    punct(Lang::En, "exclamation point", attach("!")),
    punct(Lang::En, "colon", attach(":")),
    punct(Lang::En, "semicolon", attach(";")),
    punct(Lang::En, "ellipsis", attach("\u{2026}")),
    punct(Lang::En, "open quote", open("\"")),
    punct(Lang::En, "open quotes", open("\"")),
    punct(Lang::En, "close quote", close("\"")),
    punct(Lang::En, "close quotes", close("\"")),
    punct(Lang::En, "open parenthesis", open("(")),
    punct(Lang::En, "close parenthesis", close(")")),
    // Spanish
    cmd(Lang::Es, "nueva línea", Action::Line),
    cmd(Lang::Es, "salto de línea", Action::Line),
    cmd(Lang::Es, "nuevo párrafo", Action::Paragraph),
    cmd(Lang::Es, "borra eso", Action::Scratch),
    punct_bare(Lang::Es, "coma", attach(",")),
    punct_bare(Lang::Es, "punto", attach(".")),
    punct(Lang::Es, "punto final", attach(".")),
    punct(Lang::Es, "punto y coma", attach(";")),
    punct(Lang::Es, "dos puntos", attach(":")),
    punct(Lang::Es, "puntos suspensivos", attach("\u{2026}")),
    punct(Lang::Es, "signo de interrogación", attach("?")),
    punct(Lang::Es, "signo de exclamación", attach("!")),
    punct(Lang::Es, "abre interrogación", open("\u{bf}")),
    punct(Lang::Es, "abrir interrogación", open("\u{bf}")),
    punct(Lang::Es, "cierra interrogación", close("?")),
    punct(Lang::Es, "cerrar interrogación", close("?")),
    punct(Lang::Es, "abre exclamación", open("\u{a1}")),
    punct(Lang::Es, "abrir exclamación", open("\u{a1}")),
    punct(Lang::Es, "cierra exclamación", close("!")),
    punct(Lang::Es, "cerrar exclamación", close("!")),
    punct(Lang::Es, "abrir comillas", open("\"")),
    punct(Lang::Es, "abre comillas", open("\"")),
    punct(Lang::Es, "cerrar comillas", close("\"")),
    punct(Lang::Es, "cierra comillas", close("\"")),
    punct(Lang::Es, "abrir paréntesis", open("(")),
    punct(Lang::Es, "abre paréntesis", open("(")),
    punct(Lang::Es, "cerrar paréntesis", close(")")),
    punct(Lang::Es, "cierra paréntesis", close(")")),
    // French
    cmd(Lang::Fr, "nouvelle ligne", Action::Line),
    cmd(Lang::Fr, "retour à la ligne", Action::Line),
    cmd(Lang::Fr, "nouveau paragraphe", Action::Paragraph),
    cmd(Lang::Fr, "efface ça", Action::Scratch),
    punct(Lang::Fr, "virgule", attach(",")),
    punct_bare(Lang::Fr, "point", attach(".")),
    punct(Lang::Fr, "point final", attach(".")),
    punct(Lang::Fr, "point d'interrogation", attach_fr("?")),
    punct(Lang::Fr, "point d'exclamation", attach_fr("!")),
    punct(Lang::Fr, "deux points", attach_fr(":")),
    punct(Lang::Fr, "point virgule", attach_fr(";")),
    punct(Lang::Fr, "points de suspension", attach("\u{2026}")),
    punct(Lang::Fr, "ouvrir les guillemets", open("\u{ab}")),
    punct(Lang::Fr, "ouvrez les guillemets", open("\u{ab}")),
    punct(Lang::Fr, "fermer les guillemets", close_fr("\u{bb}")),
    punct(Lang::Fr, "fermez les guillemets", close_fr("\u{bb}")),
    punct(Lang::Fr, "ouvrir la parenthèse", open("(")),
    punct(Lang::Fr, "ouvrez la parenthèse", open("(")),
    punct(Lang::Fr, "fermer la parenthèse", close(")")),
    punct(Lang::Fr, "fermez la parenthèse", close(")")),
];

/// Les phrases, mots deja prepares, les plus longues d'abord (pour que
/// "punto y coma" passe avant "punto", "point virgule" avant "point").
fn table() -> &'static [Entry] {
    static TABLE: OnceLock<Vec<Entry>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut v: Vec<Entry> = ROWS
            .iter()
            .map(|&(lang, group, explicit_only, phrase, action)| Entry {
                lang,
                group,
                explicit_only,
                keys: phrase::keys(phrase),
                action,
            })
            .collect();
        v.sort_by_key(|e| std::cmp::Reverse(e.keys.len()));
        v
    })
}

/// Langues actives pour un groupe, et si les mots trop courants sont exclus.
fn active_languages(language: Option<&str>, group: Group) -> (Vec<Lang>, bool) {
    let base = language
        .map(|l| l.trim().to_lowercase())
        .and_then(|l| l.split(['-', '_']).next().map(str::to_string))
        .filter(|l| !l.is_empty() && l != "auto");
    let own = match base.as_deref() {
        None => return (vec![Lang::En, Lang::Es, Lang::Fr], true),
        Some("en") => Some(Lang::En),
        Some("es") => Some(Lang::Es),
        Some("fr") => Some(Lang::Fr),
        Some(_) => None,
    };
    let mut langs = Vec::new();
    match group {
        Group::Commands => {
            if let Some(l) = own {
                langs.push(l);
            }
            if !langs.contains(&Lang::En) {
                langs.push(Lang::En);
            }
        }
        Group::Punctuation => langs.extend(own),
    }
    (langs, false)
}

/// Mots qui, juste avant "period", "coma", "point"..., disent que c'est un
/// nom commun ("the period", "un punto", "ce point") et pas un ordre.
const ARTICLES: &[&str] = &[
    // English
    "a", "an", "the", "this", "that", "each", "every", "same", "my", "your", "our", "his", "her", "its",
    "their", "another", "no",
    // Spanish
    "el", "la", "los", "las", "un", "una", "unos", "unas", "este", "esta", "ese", "esa", "mi", "tu", "su",
    "otro", "otra", "del", "al",
    // French
    "le", "les", "une", "ce", "cette", "mon", "ma", "ton", "ta", "son", "sa", "au", "du", "des",
];

fn is_article(key: &str) -> bool {
    ARTICLES.contains(&key)
}

/// Les phrases trouvees : (octet de debut, octet de fin, action).
fn find(text: &str, language: Option<&str>, group: Group) -> Vec<(usize, usize, Action)> {
    let (langs, skip_bare) = active_languages(language, group);
    let entries: Vec<&Entry> = table()
        .iter()
        .filter(|e| e.group == group && langs.contains(&e.lang) && !(skip_bare && e.explicit_only))
        .collect();
    if entries.is_empty() {
        return Vec::new();
    }
    let words = phrase::tokenize(text);
    let mut found = Vec::new();
    let mut i = 0;
    while i < words.len() {
        let hit = entries.iter().find_map(|e| {
            let n = phrase::matches_at(text, &words, i, &e.keys, true)?;
            // A common noun, not an order: "over a period of time".
            let noun = matches!(e.action, Action::Symbol(s) if s.kind == Kind::Attach)
                && i > 0
                && is_article(&words[i - 1].key);
            (!noun).then_some((n, e.action))
        });
        match hit {
            Some((n, action)) => {
                found.push((words[i].start, words[i + n - 1].end, action));
                i += n;
            }
            None => i += 1,
        }
    }
    found
}

/// Applique le groupe `group` a `text`. Sans phrase reconnue, le texte est
/// rendu tel quel (aucune espace n'est touchee).
pub fn apply(text: &str, language: Option<&str>, group: Group) -> String {
    let found = find(text, language, group);
    if found.is_empty() {
        return text.to_string();
    }
    let mut out = Out { s: String::with_capacity(text.len()), glue: false, cap: false };
    let mut cursor = 0;
    let mut after_command = false;
    for (start, end, action) in found {
        out.push_text(&text[cursor..start], after_command);
        out.run(action);
        cursor = end;
        after_command = true;
    }
    out.push_text(&text[cursor..], after_command);
    out.s
}

/// Fin de phrase : . ! ? ou ...
const ENDERS: [char; 4] = ['.', '!', '?', '\u{2026}'];
/// Ponctuation que le modele de parole met autour d'une phrase parlee.
const MODEL_PUNCT: [char; 6] = ['.', ',', ';', ':', '!', '?'];

struct Out {
    s: String,
    /// Le texte suivant se colle sans espace (apres "open quote").
    glue: bool,
    /// La premiere lettre du texte suivant prend une majuscule.
    cap: bool,
}

impl Out {
    /// Le texte entre deux phrases reconnues. Apres une commande, on enleve
    /// les espaces et la ponctuation que le modele a ajoutees a la phrase.
    fn push_text(&mut self, seg: &str, after_command: bool) {
        if !after_command {
            self.s.push_str(seg);
            return;
        }
        let seg = seg.trim_start_matches(|c: char| c.is_whitespace() || MODEL_PUNCT.contains(&c) || c == '\u{2026}');
        if seg.is_empty() {
            return;
        }
        if !(self.s.is_empty() || self.s.ends_with('\n') || self.glue) {
            self.s.push(' ');
        }
        self.glue = false;
        if self.cap {
            self.cap = false;
            let mut chars = seg.char_indices();
            if let Some((i, c)) = chars.find(|(_, c)| c.is_alphanumeric()) {
                self.s.push_str(&seg[..i]);
                self.s.extend(c.to_uppercase());
                self.s.push_str(&seg[i + c.len_utf8()..]);
                return;
            }
        }
        self.s.push_str(seg);
    }

    /// Enleve les espaces de fin (pas les sauts de ligne) et, si `comma`, une
    /// virgule que le modele a mise avant la phrase parlee.
    fn trim_tail(&mut self, comma: bool) {
        self.s.truncate(self.s.trim_end_matches([' ', '\t']).len());
        if comma && self.s.ends_with(',') {
            self.s.pop();
            self.s.truncate(self.s.trim_end_matches([' ', '\t']).len());
        }
    }

    fn run(&mut self, action: Action) {
        self.glue = false;
        self.cap = false;
        match action {
            // A comma before a line break stays: "Dear Ann, new line".
            Action::Line => {
                self.trim_tail(false);
                self.s.push('\n');
            }
            Action::Paragraph => {
                self.trim_tail(false);
                self.s.push_str("\n\n");
                self.cap = true;
            }
            Action::Scratch => {
                self.trim_tail(true);
                self.scratch();
            }
            Action::Symbol(sym) => self.symbol(sym),
        }
    }

    /// Retire la derniere phrase.
    fn scratch(&mut self) {
        // The sentence that was just ended ("Meet at five. Scratch that."):
        // look before its closing mark.
        while self.s.ends_with(ENDERS) {
            self.s.pop();
        }
        self.trim_tail(false);
        let mut cut = 0;
        let mut prev_end: Option<usize> = None;
        for (i, c) in self.s.char_indices() {
            // A mark ends a sentence only when a space or line break follows
            // ("3.5" and "e.g.x" do not).
            if let Some(p) = prev_end.take() {
                if c.is_whitespace() {
                    cut = p;
                }
            }
            if c == '\n' {
                cut = i + 1;
            } else if ENDERS.contains(&c) {
                prev_end = Some(i + c.len_utf8());
            }
        }
        self.s.truncate(cut);
        // What follows starts a sentence when nothing, or a finished sentence,
        // is left.
        self.cap = self.s.is_empty() || self.s.ends_with(ENDERS);
    }

    fn symbol(&mut self, sym: Symbol) {
        match sym.kind {
            Kind::Attach => {
                self.trim_tail(true);
                let sentence_end = sym.text.ends_with(['.', '!', '?']);
                if sentence_end {
                    // "Done, period" -> "Done."
                    while self.s.ends_with([',', ';', ':']) {
                        self.s.pop();
                        self.trim_tail(false);
                    }
                }
                if !self.s.ends_with(sym.text) {
                    if sym.space_before && !self.s.is_empty() && !self.s.ends_with('\n') {
                        self.s.push(' ');
                    }
                    self.s.push_str(sym.text);
                }
                self.cap = sentence_end;
            }
            Kind::Open => {
                self.trim_tail(false);
                if !self.s.is_empty() && !self.s.ends_with('\n') {
                    self.s.push(' ');
                }
                self.s.push_str(sym.text);
                // French guillemets take a space inside.
                if sym.text == "\u{ab}" {
                    self.s.push(' ');
                }
                self.glue = true;
            }
            Kind::Close => {
                self.trim_tail(true);
                if sym.space_before && !self.s.is_empty() {
                    self.s.push(' ');
                }
                self.s.push_str(sym.text);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmds(text: &str, lang: Option<&str>) -> String {
        apply(text, lang, Group::Commands)
    }
    fn punct(text: &str, lang: Option<&str>) -> String {
        apply(text, lang, Group::Punctuation)
    }

    // -- new line / new paragraph -------------------------------------------

    #[test]
    fn new_line_in_english_with_the_punctuation_a_model_adds() {
        let en = Some("en");
        assert_eq!(cmds("Hello world. New line. Next thing.", en), "Hello world.\nNext thing.");
        assert_eq!(cmds("Hello world, new line, next thing", en), "Hello world,\nnext thing");
        assert_eq!(cmds("Hello world new line next thing", en), "Hello world\nnext thing");
        assert_eq!(cmds("hello NEW LINE there", en), "hello\nthere");
        assert_eq!(cmds("Hello world. Newline. Next.", en), "Hello world.\nNext.");
    }

    #[test]
    fn new_paragraph_in_english() {
        let en = Some("en");
        assert_eq!(cmds("First part. New paragraph. Second part.", en), "First part.\n\nSecond part.");
        // The next sentence starts with a capital after a paragraph break.
        assert_eq!(cmds("first part new paragraph second part", en), "first part\n\nSecond part");
    }

    #[test]
    fn spanish_commands_ignore_accents_and_case() {
        let es = Some("es");
        assert_eq!(cmds("Hola. Nueva línea. Adiós.", es), "Hola.\nAdiós.");
        assert_eq!(cmds("hola nueva linea adiós", es), "hola\nadiós");
        assert_eq!(cmds("Uno. Nuevo párrafo. Dos.", es), "Uno.\n\nDos.");
        assert_eq!(cmds("Uno, salto de línea, dos", es), "Uno,\ndos");
    }

    #[test]
    fn french_commands() {
        let fr = Some("fr");
        assert_eq!(cmds("Bonjour. Nouvelle ligne. Merci.", fr), "Bonjour.\nMerci.");
        assert_eq!(cmds("Un. Nouveau paragraphe. Deux.", fr), "Un.\n\nDeux.");
        assert_eq!(cmds("Un, retour à la ligne, deux", fr), "Un,\ndeux");
    }

    #[test]
    fn english_commands_always_work_and_other_languages_do_not_mix() {
        // A Spanish dictation still understands "new line"...
        assert_eq!(cmds("Hola. New line. Adiós.", Some("es")), "Hola.\nAdiós.");
        assert_eq!(cmds("Bonjour new line merci", Some("fr")), "Bonjour\nmerci");
        // ... but not the French phrase.
        assert_eq!(cmds("Hola. Nouvelle ligne. Adiós.", Some("es")), "Hola. Nouvelle ligne. Adiós.");
        assert_eq!(cmds("Hello. Nueva línea. Bye.", Some("en")), "Hello. Nueva línea. Bye.");
        // An unknown language gets English only.
        assert_eq!(cmds("Hallo new line Welt", Some("de")), "Hallo\nWelt");
        assert_eq!(cmds("Hallo nueva línea Welt", Some("de")), "Hallo nueva línea Welt");
    }

    #[test]
    fn automatic_language_understands_all_three() {
        assert_eq!(cmds("One new line two", None), "One\ntwo");
        assert_eq!(cmds("Uno nueva línea dos", None), "Uno\ndos");
        assert_eq!(cmds("Un nouvelle ligne deux", None), "Un\ndeux");
        assert_eq!(cmds("Un nouvelle ligne deux", Some("auto")), "Un\ndeux");
        assert_eq!(cmds("Un nouvelle ligne deux", Some("  ")), "Un\ndeux");
    }

    #[test]
    fn language_codes_with_a_region_are_accepted() {
        assert_eq!(cmds("Hola nueva línea adiós", Some("es-MX")), "Hola\nadiós");
        assert_eq!(cmds("Bonjour nouvelle ligne merci", Some("fr_CA")), "Bonjour\nmerci");
    }

    #[test]
    fn a_command_at_the_start_or_end() {
        let en = Some("en");
        assert_eq!(cmds("New line. Hello", en), "\nHello");
        assert_eq!(cmds("Hello. New line.", en), "Hello.\n");
        assert_eq!(cmds("Hello new line", en), "Hello\n");
        assert_eq!(cmds("New paragraph", en), "\n\n");
    }

    #[test]
    fn repeated_commands_stack() {
        let en = Some("en");
        assert_eq!(cmds("A. New line. New line. B.", en), "A.\n\nB.");
        assert_eq!(cmds("A new paragraph new paragraph b", en), "A\n\n\n\nB");
        assert_eq!(cmds("A new line new paragraph b", en), "A\n\n\nB");
    }

    #[test]
    fn spaces_around_the_inserted_newline_are_cleaned() {
        assert_eq!(cmds("Hello   world.   New line.   Next", Some("en")), "Hello   world.\nNext");
        assert_eq!(cmds("Hello \t new line \t next", Some("en")), "Hello\nnext");
    }

    #[test]
    fn text_without_a_command_is_returned_untouched() {
        let t = "  Hello   world.  \n Second line.  ";
        assert_eq!(cmds(t, Some("en")), t);
        assert_eq!(cmds("", Some("en")), "");
    }

    #[test]
    fn words_that_merely_contain_the_phrase_are_left_alone() {
        let en = Some("en");
        assert_eq!(cmds("The headline and the baseline", en), "The headline and the baseline");
        assert_eq!(cmds("renew lines", en), "renew lines");
        assert_eq!(cmds("a news line", en), "a news line");
        assert_eq!(cmds("scratched that", en), "scratched that");
        assert_eq!(cmds("scratch thatch", en), "scratch thatch");
        assert_eq!(cmds("nuevalinea", Some("es")), "nuevalinea");
    }

    #[test]
    fn a_phrase_split_by_other_punctuation_is_not_a_command() {
        assert_eq!(cmds("new. line", Some("en")), "new. line");
        assert_eq!(cmds("Is it new? Line two", Some("en")), "Is it new? Line two");
    }

    #[test]
    fn a_comma_inside_the_phrase_is_tolerated() {
        assert_eq!(cmds("Hello new, line next", Some("en")), "Hello\nnext");
    }

    // -- scratch that -------------------------------------------------------

    #[test]
    fn scratch_that_removes_the_sentence_before_it() {
        let en = Some("en");
        assert_eq!(cmds("I like cats. Meet at five, scratch that.", en), "I like cats.");
        assert_eq!(cmds("I like cats. Meet at five scratch that Meet at six.", en), "I like cats. Meet at six.");
        assert_eq!(cmds("Hello. Scratch that. Goodbye.", en), "Goodbye.");
    }

    #[test]
    fn scratch_that_removes_the_sentence_that_just_ended() {
        let en = Some("en");
        // The model closed the sentence before the command.
        assert_eq!(cmds("One. Two. Scratch that. Three.", en), "One. Three.");
        assert_eq!(cmds("One? Two! Scratch that.", en), "One?");
    }

    #[test]
    fn scratch_that_with_nothing_before_does_nothing() {
        let en = Some("en");
        assert_eq!(cmds("Scratch that. Hello.", en), "Hello.");
        assert_eq!(cmds("Scratch that", en), "");
        assert_eq!(cmds("Scratch that scratch that", en), "");
    }

    #[test]
    fn repeated_scratch_that_removes_one_sentence_each() {
        let en = Some("en");
        assert_eq!(cmds("A one. B two. C three. Scratch that. Scratch that.", en), "A one.");
        assert_eq!(cmds("A one. B two. Scratch that. Scratch that. Scratch that.", en), "");
    }

    #[test]
    fn scratch_that_stops_at_a_line_break() {
        let en = Some("en");
        assert_eq!(cmds("Line one. New line. Line two scratch that Line three.", en), "Line one.\nLine three.");
        // Right after a break there is nothing on the line to remove.
        assert_eq!(cmds("Line one. New line. Scratch that. Line two.", en), "Line one.\nLine two.");
    }

    #[test]
    fn scratch_that_does_not_stop_at_a_decimal_point_or_abbreviation() {
        let en = Some("en");
        assert_eq!(cmds("Costs 3.5 euros scratch that", en), "");
        assert_eq!(cmds("Done. It costs 3.5 euros scratch that", en), "Done.");
    }

    #[test]
    fn scratch_that_in_spanish_and_french() {
        assert_eq!(cmds("Uno. Dos. Borra eso. Tres.", Some("es")), "Uno. Tres.");
        assert_eq!(cmds("Un. Deux. Efface ça. Trois.", Some("fr")), "Un. Trois.");
        assert_eq!(cmds("Un. Deux, efface ca", Some("fr")), "Un.");
    }

    #[test]
    fn what_follows_scratch_that_starts_a_sentence() {
        assert_eq!(cmds("Hello there scratch that and then goodbye", Some("en")), "And then goodbye");
    }

    // -- spoken punctuation -------------------------------------------------

    #[test]
    fn english_punctuation_words() {
        let en = Some("en");
        assert_eq!(punct("Hello comma how are you question mark", en), "Hello, how are you?");
        assert_eq!(punct("I am done period Next", en), "I am done. Next");
        assert_eq!(punct("Wow exclamation mark", en), "Wow!");
        assert_eq!(punct("Wow exclamation point", en), "Wow!");
        assert_eq!(punct("Note colon milk semicolon eggs full stop", en), "Note: milk; eggs.");
        assert_eq!(punct("Well ellipsis maybe", en), "Well… maybe");
    }

    #[test]
    fn punctuation_words_with_the_punctuation_a_model_adds() {
        let en = Some("en");
        assert_eq!(punct("Hello, comma, how are you? Question mark.", en), "Hello, how are you?");
        assert_eq!(punct("I am done. Period.", en), "I am done.");
        assert_eq!(punct("I am done period.", en), "I am done.");
    }

    #[test]
    fn the_next_word_after_a_spoken_period_gets_a_capital() {
        assert_eq!(punct("one period two", Some("en")), "one. Two");
        assert_eq!(punct("one comma two", Some("en")), "one, two");
    }

    #[test]
    fn spoken_quotes_and_parentheses() {
        let en = Some("en");
        assert_eq!(punct("He said open quote hello close quote today", en), "He said \"hello\" today");
        assert_eq!(punct("He said comma open quote hello close quote", en), "He said, \"hello\"");
        assert_eq!(punct("Call me open parenthesis later close parenthesis ok", en), "Call me (later) ok");
    }

    #[test]
    fn spanish_punctuation() {
        let es = Some("es");
        assert_eq!(punct("Hola coma qué tal", es), "Hola, qué tal");
        assert_eq!(punct("Listo punto Siguiente", es), "Listo. Siguiente");
        assert_eq!(punct("Listo punto final", es), "Listo.");
        assert_eq!(punct("Uno punto y coma dos dos puntos tres", es), "Uno; dos: tres");
        assert_eq!(punct("abre interrogación cómo estás cierra interrogación", es), "¿cómo estás?");
        assert_eq!(punct("abre exclamación hola cierra exclamación", es), "¡hola!");
        assert_eq!(punct("Dijo abrir comillas hola cerrar comillas", es), "Dijo \"hola\"");
    }

    #[test]
    fn french_punctuation_with_a_space_before_the_double_marks() {
        let fr = Some("fr");
        assert_eq!(punct("Bonjour virgule comment ça va point d'interrogation", fr), "Bonjour, comment ça va ?");
        assert_eq!(punct("Super point d'exclamation", fr), "Super !");
        assert_eq!(punct("Notez deux points lait point virgule oeufs point final", fr), "Notez : lait ; oeufs.");
        assert_eq!(punct("Il dit ouvrir les guillemets salut fermer les guillemets", fr), "Il dit « salut »");
        assert_eq!(punct("Fini point Suite", fr), "Fini. Suite");
        assert_eq!(punct("Fini point-virgule suite", fr), "Fini ; suite");
    }

    #[test]
    fn punctuation_words_are_for_the_dictation_language_only() {
        assert_eq!(punct("Hola period adiós", Some("es")), "Hola period adiós");
        assert_eq!(punct("Hello coma bye", Some("en")), "Hello coma bye");
        assert_eq!(punct("Hallo comma Welt", Some("de")), "Hallo comma Welt");
    }

    #[test]
    fn automatic_language_skips_the_words_that_are_too_common_alone() {
        // "punto", "coma" and "point" on their own are everyday words.
        assert_eq!(punct("un punto de vista", None), "un punto de vista");
        assert_eq!(punct("fini point suite", None), "fini point suite");
        // The unambiguous ones work in any language.
        assert_eq!(punct("Hello comma bye", None), "Hello, bye");
        assert_eq!(punct("Uno punto y coma dos", None), "Uno; dos");
        assert_eq!(punct("Super point d'exclamation", None), "Super !");
    }

    #[test]
    fn a_noun_is_not_an_order() {
        let en = Some("en");
        assert_eq!(punct("over a period of time", en), "over a period of time");
        assert_eq!(punct("the comma key", en), "the comma key");
        assert_eq!(punct("la coma del paciente", Some("es")), "la coma del paciente");
        assert_eq!(punct("un punto de vista", Some("es")), "un punto de vista");
        assert_eq!(punct("ce point de vue", Some("fr")), "ce point de vue");
        // Not after an article: still an order.
        assert_eq!(punct("that is all period", en), "that is all.");
    }

    #[test]
    fn punctuation_words_inside_other_words_are_left_alone() {
        assert_eq!(punct("periodic table, commanding", Some("en")), "periodic table, commanding");
    }

    #[test]
    fn the_two_groups_are_independent() {
        let t = "Hello comma world new line next";
        assert_eq!(cmds(t, Some("en")), "Hello comma world\nnext");
        assert_eq!(punct(t, Some("en")), "Hello, world new line next");
        assert_eq!(cmds(&punct(t, Some("en")), Some("en")), "Hello, world\nnext");
    }

    #[test]
    fn punctuation_then_commands_together() {
        let t = "Dear Ann comma new line thanks for the note period Scratch that Bye period";
        let out = cmds(&punct(t, Some("en")), Some("en"));
        assert_eq!(out, "Dear Ann,\nBye.");
    }
}
