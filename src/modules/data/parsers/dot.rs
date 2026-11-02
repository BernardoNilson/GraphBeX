use std::borrow::Cow;

use super::scanner::Scanner;
use super::{GraphParser, ParseError, edge_weight};
use crate::modules::common::graph::DEFAULT_EDGE_WEIGHT;
use crate::modules::data::graph_builder::GraphBuilder;

/// How deeply subgraphs may nest. Each level is a recursive call.
const MAX_NESTING: usize = 256;

/// Reads DOT files, the language of Graphviz, by recursive descent:
///
/// ```text
/// graph     : [strict] (graph | digraph) [ID] '{' stmt_list '}'
/// stmt      : node_stmt | edge_stmt | attr_stmt | ID '=' ID | subgraph
/// node_stmt : node_id [attr_list]
/// edge_stmt : (node_id | subgraph) (edgeop (node_id | subgraph))+ [attr_list]
/// subgraph  : [subgraph [ID]] '{' stmt_list '}'
/// ```
///
/// `a -- b -- c` yields a-b and b-c, and `a -- {b c}` yields a-b and a-c.
/// `--` and `->` both work, since the graph is undirected. Of the edge
/// attributes only `weight` is read. Attribute statements and ports are
/// skipped, and only the first graph of the file is read.
pub struct DotParser;

impl GraphParser for DotParser {
    fn parse(&self, bytes: &[u8]) -> Result<GraphBuilder, ParseError> {
        let mut document = DotDocument {
            tokens: Tokenizer::new(bytes),
            builder: GraphBuilder::default(),
            groups: Vec::new(),
        };

        document.graph()?;
        Ok(document.builder)
    }
}

/// The words with a meaning of their own, compared ignoring case.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Keyword {
    Strict,
    Graph,
    Digraph,
    Node,
    Edge,
    Subgraph,
}

impl Keyword {
    fn parse(word: &[u8]) -> Option<Self> {
        const KEYWORDS: [(&[u8], Keyword); 6] = [
            (b"strict", Keyword::Strict),
            (b"graph", Keyword::Graph),
            (b"digraph", Keyword::Digraph),
            (b"node", Keyword::Node),
            (b"edge", Keyword::Edge),
            (b"subgraph", Keyword::Subgraph),
        ];

        KEYWORDS
            .iter()
            .find(|(text, _)| word.eq_ignore_ascii_case(text))
            .map(|&(_, keyword)| keyword)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Ahead {
    End,
    Keyword(Keyword),
    Id,
    EdgeOp,
    Symbol(u8),
}

type Attribute<'a> = (Cow<'a, [u8]>, Cow<'a, [u8]>, usize);

struct DotDocument<'a> {
    tokens: Tokenizer<'a>,
    builder: GraphBuilder,
    /// The vertices of each subgraph being read, innermost last.
    groups: Vec<Vec<u32>>,
}

impl<'a> DotDocument<'a> {
    fn graph(&mut self) -> Result<(), ParseError> {
        self.skip_if(Ahead::Keyword(Keyword::Strict))?;

        match self.ahead()? {
            Ahead::Keyword(Keyword::Graph | Keyword::Digraph) => self.tokens.next()?,
            _ => return Err(self.error("expected 'graph' or 'digraph'")),
        };

        self.skip_if(Ahead::Id)?;
        self.expect(b'{')?;
        self.statements()?;
        self.expect(b'}')
    }

    /// The statements up to the `}` that closes them, which is left unread.
    fn statements(&mut self) -> Result<(), ParseError> {
        loop {
            match self.ahead()? {
                Ahead::End => return Err(self.error("a '{' is never closed")),
                Ahead::Symbol(b'}') => return Ok(()),
                Ahead::Symbol(b';') => {
                    self.tokens.next()?;
                }
                _ => self.statement()?,
            }
        }
    }

    fn statement(&mut self) -> Result<(), ParseError> {
        match self.ahead()? {
            // Defaults for what follows, which do not change the topology.
            Ahead::Keyword(Keyword::Graph | Keyword::Node | Keyword::Edge) => {
                self.tokens.next()?;
                self.attributes().map(drop)
            }

            Ahead::Keyword(Keyword::Subgraph) | Ahead::Symbol(b'{') => {
                let vertices = self.subgraph()?;
                self.edges_from(vertices)
            }

            Ahead::Id => {
                let id = self.id()?;

                // `ID = ID` sets an attribute of the graph.
                if self.skip_if(Ahead::Symbol(b'='))? {
                    return self.id().map(drop);
                }

                let vertex = self.vertex(&id)?;
                self.edges_from(vec![vertex])
            }

            _ => Err(self.error("expected a statement")),
        }
    }

    /// The rest of a statement that started with a vertex or a subgraph: the
    /// edge chain, if any, and its attributes. Without an edge, the
    /// attributes are the vertices'.
    fn edges_from(&mut self, first: Vec<u32>) -> Result<(), ParseError> {
        let mut operands = vec![first];

        while self.skip_if(Ahead::EdgeOp)? {
            operands.push(self.operand()?);
        }

        let attributes = self.attributes()?;

        if operands.len() < 2 {
            for &vertex in &operands[0] {
                for (name, value, _) in &attributes {
                    self.builder.set_attribute(vertex, name, value);
                }
            }
            return Ok(());
        }

        let weight = match attributes.iter().find(|(name, ..)| name.as_ref() == b"weight") {
            Some((_, raw, line)) => edge_weight(raw, *line)?,
            None => DEFAULT_EDGE_WEIGHT,
        };

        for pair in operands.windows(2) {
            for &source in &pair[0] {
                for &target in &pair[1] {
                    self.builder.add_edge(source, target, weight);
                }
            }
        }

        Ok(())
    }

    /// An endpoint of an edge: a vertex, or every vertex of a subgraph.
    fn operand(&mut self) -> Result<Vec<u32>, ParseError> {
        match self.ahead()? {
            Ahead::Keyword(Keyword::Subgraph) | Ahead::Symbol(b'{') => self.subgraph(),
            Ahead::Id => {
                let id = self.id()?;
                Ok(vec![self.vertex(&id)?])
            }
            _ => Err(self.error("expected a vertex or a subgraph after the edge")),
        }
    }

    fn subgraph(&mut self) -> Result<Vec<u32>, ParseError> {
        if self.skip_if(Ahead::Keyword(Keyword::Subgraph))? {
            self.skip_if(Ahead::Id)?;
        }

        if self.groups.len() >= MAX_NESTING {
            return Err(self.error("subgraphs are nested too deeply"));
        }

        self.expect(b'{')?;
        self.groups.push(Vec::new());
        self.statements()?;
        self.expect(b'}')?;

        let group = self.groups.pop().expect("the group was pushed above");

        // A subgraph inside another belongs to it as well.
        if let Some(parent) = self.groups.last_mut() {
            parent.extend_from_slice(&group);
        }

        Ok(group)
    }

    /// The attribute lists `[a=b, c=d][e=f]` as `(name, value, line)`.
    /// Attributes without a value are skipped.
    fn attributes(&mut self) -> Result<Vec<Attribute<'a>>, ParseError> {
        let mut attributes = Vec::new();

        while self.skip_if(Ahead::Symbol(b'['))? {
            loop {
                match self.ahead()? {
                    Ahead::Symbol(b']') => {
                        self.tokens.next()?;
                        break;
                    }
                    Ahead::Symbol(b',' | b';') => {
                        self.tokens.next()?;
                    }
                    Ahead::Id | Ahead::Keyword(_) => {
                        let key = self.id()?;
                        if self.skip_if(Ahead::Symbol(b'='))? {
                            let value = self.id()?;
                            attributes.push((key, value, self.tokens.line));
                        }
                    }
                    Ahead::End => return Err(self.error("a '[' is never closed")),
                    _ => return Err(self.error("expected an attribute")),
                }
            }
        }

        Ok(attributes)
    }

    /// An identifier, joining quoted strings concatenated with `+`.
    fn id(&mut self) -> Result<Cow<'a, [u8]>, ParseError> {
        let Some(Token::Id { mut text, quoted }) = self.tokens.next()? else {
            return Err(self.error("expected an identifier"));
        };

        while quoted && self.skip_if(Ahead::Symbol(b'+'))? {
            let Some(Token::Id { text: more, quoted: true }) = self.tokens.next()? else {
                return Err(self.error("expected a quoted string after '+'"));
            };
            text.to_mut().extend_from_slice(&more);
        }

        Ok(text)
    }

    /// The index of a vertex, which also joins the subgraph being read. The
    /// port that may follow it (`:port:compass`) is skipped.
    fn vertex(&mut self, id: &[u8]) -> Result<u32, ParseError> {
        for _ in 0..2 {
            if !self.skip_if(Ahead::Symbol(b':'))? {
                break;
            }
            self.id()?;
        }

        let vertex = self.builder.vertex(id);
        if let Some(group) = self.groups.last_mut() {
            group.push(vertex);
        }

        Ok(vertex)
    }

    fn expect(&mut self, symbol: u8) -> Result<(), ParseError> {
        match self.tokens.next()? {
            Some(Token::Symbol(found)) if found == symbol => Ok(()),
            _ => Err(self.error(format!("expected '{}'", char::from(symbol)))),
        }
    }

    fn skip_if(&mut self, kind: Ahead) -> Result<bool, ParseError> {
        let matches = self.ahead()? == kind;
        if matches {
            self.tokens.next()?;
        }
        Ok(matches)
    }

    fn ahead(&mut self) -> Result<Ahead, ParseError> {
        Ok(match self.tokens.peek()? {
            None => Ahead::End,
            Some(Token::EdgeOp) => Ahead::EdgeOp,
            Some(Token::Symbol(symbol)) => Ahead::Symbol(*symbol),
            // Only an unquoted word can be a keyword: "graph" is a vertex.
            Some(Token::Id { text, quoted: false }) => {
                Keyword::parse(text).map_or(Ahead::Id, Ahead::Keyword)
            }
            Some(Token::Id { .. }) => Ahead::Id,
        })
    }

    fn error(&self, reason: impl Into<String>) -> ParseError {
        ParseError::new(self.tokens.line, reason)
    }
}

enum Token<'a> {
    /// An identifier, numeral, quoted or HTML string. The text is only copied
    /// when a quoted string had escapes.
    Id { text: Cow<'a, [u8]>, quoted: bool },
    /// `--` or `->`.
    EdgeOp,
    /// One of `{ } [ ] = ; , : +`.
    Symbol(u8),
}

/// Splits the bytes into tokens, with one token of lookahead.
struct Tokenizer<'a> {
    scanner: Scanner<'a>,
    /// The line where the last token read, or peeked, starts.
    line: usize,
    /// A peeked token. `Some(None)` is a peeked end of file.
    peeked: Option<Option<Token<'a>>>,
}

impl<'a> Tokenizer<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            scanner: Scanner::new(bytes),
            line: 1,
            peeked: None,
        }
    }

    fn next(&mut self) -> Result<Option<Token<'a>>, ParseError> {
        match self.peeked.take() {
            Some(token) => Ok(token),
            None => self.read(),
        }
    }

    fn peek(&mut self) -> Result<Option<&Token<'a>>, ParseError> {
        if self.peeked.is_none() {
            self.peeked = Some(self.read()?);
        }
        Ok(self.peeked.as_ref().and_then(Option::as_ref))
    }

    fn read(&mut self) -> Result<Option<Token<'a>>, ParseError> {
        let scanner = &mut self.scanner;
        skip_blanks(scanner)?;
        self.line = scanner.line;

        let Some(byte) = scanner.peek(0) else {
            return Ok(None);
        };

        let id = |text, quoted| Some(Token::Id { text, quoted });

        Ok(match byte {
            b'-' if matches!(scanner.peek(1), Some(b'-' | b'>')) => {
                scanner.skip(2);
                Some(Token::EdgeOp)
            }
            b'{' | b'}' | b'[' | b']' | b'=' | b';' | b',' | b':' | b'+' => {
                scanner.skip(1);
                Some(Token::Symbol(byte))
            }
            b'"' => id(quoted(scanner)?, true),
            b'<' => id(Cow::Borrowed(html(scanner)?), true),
            byte if is_name_start(byte) => id(Cow::Borrowed(scanner.take_while(is_name_part)), false),
            b'-' | b'.' | b'0'..=b'9' => id(Cow::Borrowed(numeral(scanner)?), false),
            _ => {
                let reason = format!("unexpected character '{}'", char::from(byte));
                return Err(ParseError::new(scanner.line, reason));
            }
        })
    }
}

/// Skips whitespace and comments: `// ...`, `/* ... */` and lines starting
/// with `#`, the output of the C preprocessor.
fn skip_blanks(scanner: &mut Scanner) -> Result<(), ParseError> {
    loop {
        scanner.skip_whitespace();

        match (scanner.peek(0), scanner.peek(1)) {
            (Some(b'#'), _) | (Some(b'/'), Some(b'/')) => scanner.skip_line(),
            (Some(b'/'), Some(b'*')) => {
                let line = scanner.line;
                scanner.skip(2);
                scanner
                    .take_until(b"*/")
                    .ok_or_else(|| ParseError::new(line, "a comment is never closed"))?;
            }
            _ => return Ok(()),
        }
    }
}

/// A double-quoted string. As in Graphviz, `\"` stands for a quote and a
/// backslash before a line break joins the lines. Other backslashes stay.
fn quoted<'a>(scanner: &mut Scanner<'a>) -> Result<Cow<'a, [u8]>, ParseError> {
    let line = scanner.line;
    scanner.skip(1);
    let start = scanner.position();
    let mut escaped = false;

    loop {
        match (scanner.peek(0), scanner.peek(1)) {
            (None, _) => return Err(ParseError::new(line, "a string is never closed")),
            (Some(b'"'), _) => break,
            (Some(b'\\'), Some(b'"' | b'\n' | b'\r')) => {
                escaped = true;
                scanner.skip(2);
            }
            _ => scanner.skip(1),
        }
    }

    let raw = scanner.since(start);
    scanner.skip(1);

    Ok(if escaped { Cow::Owned(unescape(raw)) } else { Cow::Borrowed(raw) })
}

/// An HTML string, `<...>`, whose angle brackets nest.
fn html<'a>(scanner: &mut Scanner<'a>) -> Result<&'a [u8], ParseError> {
    let line = scanner.line;
    scanner.skip(1);
    let start = scanner.position();
    let mut depth = 1;

    loop {
        match scanner.peek(0) {
            None => return Err(ParseError::new(line, "an HTML string is never closed")),
            Some(b'<') => depth += 1,
            Some(b'>') if depth == 1 => break,
            Some(b'>') => depth -= 1,
            _ => (),
        }
        scanner.skip(1);
    }

    let content = scanner.since(start);
    scanner.skip(1);
    Ok(content)
}

/// A numeral such as `42`, `-3.5` or `.5`.
fn numeral<'a>(scanner: &mut Scanner<'a>) -> Result<&'a [u8], ParseError> {
    let start = scanner.position();
    if scanner.peek(0) == Some(b'-') {
        scanner.skip(1);
    }

    if scanner.take_while(|byte| byte.is_ascii_digit() || byte == b'.').is_empty() {
        return Err(ParseError::new(scanner.line, "unexpected character '-'"));
    }

    Ok(scanner.since(start))
}

/// Bytes above ASCII are accepted, as in Graphviz, for names in UTF-8.
fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte >= 0x80
}

fn is_name_part(byte: u8) -> bool {
    is_name_start(byte) || byte.is_ascii_digit()
}

/// `\"` becomes a quote, and a backslash before a line break goes with it.
fn unescape(raw: &[u8]) -> Vec<u8> {
    let mut text = Vec::with_capacity(raw.len());
    let mut index = 0;

    while index < raw.len() {
        match (raw[index], raw.get(index + 1)) {
            (b'\\', Some(b'"')) => {
                text.push(b'"');
                index += 2;
            }
            (b'\\', Some(b'\n')) => index += 2,
            (b'\\', Some(b'\r')) if raw.get(index + 2) == Some(&b'\n') => index += 3,
            (byte, _) => {
                text.push(byte);
                index += 1;
            }
        }
    }

    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::common::graph::Graph;
    use std::sync::Arc;

    fn parse(text: &str) -> Arc<dyn Graph> {
        DotParser.parse(text.as_bytes()).unwrap().build().0
    }

    fn error_line(text: &str) -> usize {
        DotParser.parse(text.as_bytes()).unwrap_err().line
    }

    #[test]
    fn reads_an_undirected_graph() {
        let graph = parse("graph G { a -- b; b -- c; c -- a }");

        assert_eq!(graph.node_count(), 3);
        assert_eq!(graph.edge_count(), 3);
        assert_eq!(graph.neighbors(0), &[1, 2]);
        assert_eq!(graph.neighbor_weights(0), None);
    }

    #[test]
    fn reads_a_digraph_as_undirected() {
        let graph = parse("strict digraph { a -> b; b -> a }");

        assert_eq!(graph.edge_count(), 1);
        assert_eq!(graph.neighbors(1), &[0]);
    }

    #[test]
    fn a_chain_connects_each_vertex_to_the_next() {
        let graph = parse("graph { a -- b -- c }");

        assert_eq!(graph.edge_count(), 2);
        assert!(!graph.has_edge(0, 2));
    }

    #[test]
    fn an_edge_to_a_subgraph_reaches_every_vertex_in_it() {
        let graph = parse("graph { a -- { b c } }");

        assert_eq!(graph.edge_count(), 2);
        assert_eq!(graph.neighbors(0), &[1, 2]);
        assert!(!graph.has_edge(1, 2));
    }

    #[test]
    fn a_nested_subgraph_belongs_to_the_outer_one() {
        let graph = parse("graph { x -- subgraph outer { a subgraph inner { b } } }");

        assert_eq!(graph.neighbors(0), &[1, 2]);
    }

    #[test]
    fn keeps_a_vertex_declared_without_edges() {
        let graph = parse("graph { alone; a -- b }");

        assert_eq!(graph.node_count(), 3);
        assert!(graph.neighbors(0).is_empty());
    }

    #[test]
    fn reads_the_weight_among_other_attributes_and_applies_it_to_a_chain() {
        let graph = parse(r#"graph { a -- b [color=red, weight=2.5]; b -- c [label="x"][weight=3] }"#);
        assert_eq!(graph.neighbor_weights(1), Some(&[2.5, 3.0][..]));

        let chain = parse("graph { a -- b -- c [weight=2] }");
        assert_eq!(chain.neighbor_weights(1), Some(&[2.0, 2.0][..]));
    }

    #[test]
    fn ignores_the_weight_of_a_node() {
        assert_eq!(parse("graph { a [weight=heavy]; a -- b }").neighbor_weights(0), None);
    }

    #[test]
    fn reads_vertex_attributes() {
        let text = "graph { a [group=1]; b [group=2, label=x]; a -- b [group=9] }";
        let (_, attributes) = DotParser.parse(text.as_bytes()).unwrap().build();

        assert_eq!(attributes.0[0].name, "group");
        assert_eq!(attributes.0[0].categories, vec![0, 1]);
        assert_eq!(attributes.0[1].name, "label");
    }

    #[test]
    fn skips_attribute_statements_graph_attributes_ports_and_comments() {
        let graph = parse(
            "# preprocessor
             graph { rankdir=LR; node [shape=box]; edge [color=gray]; graph [splines=true]
                     // line comment
                     a:p1:n /* block
                     */ -- b:sw }",
        );

        assert_eq!(graph.node_count(), 2);
        assert_eq!(graph.edge_count(), 1);
    }

    #[test]
    fn keywords_ignore_case_but_quoted_words_are_names() {
        assert_eq!(parse(r#"DiGraph { "node" -> "graph" }"#).node_count(), 2);
    }

    #[test]
    fn quoted_and_unquoted_names_are_the_same_vertex() {
        let graph = parse(r#"graph { "a" -- b; a -- "b" }"#);

        assert_eq!(graph.node_count(), 2);
        assert_eq!(graph.edge_count(), 1);
    }

    #[test]
    fn resolves_escapes_and_concatenation() {
        let graph = parse(r#"graph { "x" + "y" -- xy; "say \"hi\"" -- "say \"hi\"" }"#);

        // Both edges are self-loops: the names on each side are the same.
        assert_eq!(graph.node_count(), 2);
        assert_eq!(graph.neighbors(0), &[0]);
        assert_eq!(graph.neighbors(1), &[1]);
    }

    #[test]
    fn reads_numerals_and_html_strings_as_names() {
        let graph = parse("graph { 1 -- -2.5 -- .5; 1 [label=<<b>one</b>>] }");

        assert_eq!(graph.node_count(), 3);
        assert_eq!(graph.edge_count(), 2);
    }

    #[test]
    fn reports_errors_at_their_line() {
        assert_eq!(error_line("graph {\n a -- b\n [weight=heavy] }"), 3);
        assert_eq!(error_line("graph {\n a -- b\n"), 3);
        assert_eq!(error_line("graph {\n a -- ; }"), 2);
    }

    #[test]
    fn rejects_a_file_that_is_not_a_graph() {
        for text in ["", "tree { a -- b }", "graph { \"open }", "graph { a -- b & c }", "graph { /* }"] {
            assert!(DotParser.parse(text.as_bytes()).is_err(), "{text}");
        }
    }

    #[test]
    fn refuses_subgraphs_nested_too_deeply() {
        let deep = format!("graph {{ {} }}", "{".repeat(MAX_NESTING + 1));

        assert!(DotParser.parse(deep.as_bytes()).is_err());
    }
}
