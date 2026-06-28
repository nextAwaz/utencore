use std::fmt;

// ── Token Kind ──

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Keywords
    KwDef, KwIf, KwElif, KwElse, KwWhile, KwFor, KwIn, KwReturn,
    KwBreak, KwContinue, KwPass, KwImport, KwFrom, KwAs, KwClass,
    KwTry, KwExcept, KwFinally, KwRaise, KwWith, KwYield, KwLambda,
    KwAnd, KwOr, KwNot, KwIs, KwNone, KwTrue, KwFalse, KwGlobal,
    KwNonlocal, KwDel, KwAssert, KwAsync, KwAwait, KwMatch, KwCase,

    // Operators
    Plus, Minus, Star, Slash, Percent, DoubleStar,
    Eq, PlusEq, MinusEq, StarEq, SlashEq, PercentEq, DoubleStarEq,
    EqEq, NotEq, Lt, Gt, LtE, GtE,
    And, Or, Not,
    BitAnd, BitOr, BitXor, BitNot, LShift, RShift,
    Walrus, Arrow, Ellipsis, At,
    Is, IsNot, InKw, NotIn,
    Pipe, FloorDiv,

    // Delimiters
    LParen, RParen, LBracket, RBracket, LBrace, RBrace,
    Comma, Colon, Semicolon, Dot, Newline, Indent, Dedent,
    EndOfFile,

    // Literals
    Name(String),
    Int,
    Float,
    String(String),
    FStringStart,
    FStringMiddle(String),
    FStringExpr,
    FStringEnd(String),

    // Special
    Comment,
    TypeComment,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub lexeme: String,
    pub line: usize,
    pub col: usize,
    pub string_id: Option<u32>,
    pub int_value: Option<i64>,
    pub float_value: Option<f64>,
}

pub struct Tokenizer {
    src: Vec<char>,
    pos: usize,
    line: usize,
    col: usize,
    indent_stack: Vec<usize>,
    pending_token: Option<Token>,
    at_line_start: bool,
    paren_depth: usize,
}

impl Tokenizer {
    pub fn new(source: &str) -> Self {
        Tokenizer {
            src: source.chars().collect(),
            pos: 0,
            line: 1,
            col: 0,
            indent_stack: vec![0],
            pending_token: None,
            at_line_start: true,
            paren_depth: 0,
        }
    }

    fn peek_char(&self) -> Option<char> {
        self.src.get(self.pos).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.src.get(self.pos).copied();
        if let Some(c) = ch {
            self.pos += 1;
            self.col += 1;
            if c == '\n' {
                self.line += 1;
                self.col = 0;
            }
        }
        ch
    }

    fn make_token(&self, kind: TokenKind, lexeme: &str) -> Token {
        Token {
            kind,
            lexeme: lexeme.to_string(),
            line: self.line,
            col: self.col.saturating_sub(lexeme.len()),
            string_id: None,
            int_value: None,
            float_value: None,
        }
    }

    fn read_identifier(&mut self) -> Token {
        let start_col = self.col;
        let mut s = String::new();
        while let Some(ch) = self.peek_char() {
            if ch.is_alphanumeric() || ch == '_' {
                s.push(ch);
                self.advance();
            } else {
                break;
            }
        }
        let kind = keyword(&s).unwrap_or_else(|| TokenKind::Name(s.clone()));
        Token {
            kind,
            lexeme: s,
            line: self.line,
            col: start_col,
            string_id: None,
            int_value: None,
            float_value: None,
        }
    }

    fn read_number(&mut self, first: char) -> Token {
        let start_col = self.col - 1;
        let mut num_str = String::new();
        num_str.push(first);
        let mut is_float = false;
        while let Some(ch) = self.peek_char() {
            if ch.is_ascii_digit() || ch == '_' || ch == '.' || ch == 'e' || ch == 'E' || ch == '+' || ch == '-' || ch == 'x' || ch == 'X' || ch.is_ascii_hexdigit() {
                if ch == '.' || ch == 'e' || ch == 'E' { is_float = true; }
                num_str.push(ch);
                self.advance();
            } else {
                break;
            }
        }
        if is_float {
            let val = num_str.replace('_', "").parse::<f64>().unwrap_or(0.0);
            Token { kind: TokenKind::Float, lexeme: num_str, line: self.line, col: start_col, string_id: None, int_value: None, float_value: Some(val) }
        } else {
            let val = num_str.replace('_', "").parse::<i64>().unwrap_or(0);
            Token { kind: TokenKind::Int, lexeme: num_str, line: self.line, col: start_col, string_id: None, int_value: Some(val), float_value: None }
        }
    }

    fn read_string(&mut self, quote: char, prefix: &str) -> Token {
        let start_col = self.col - prefix.len() - 1;
        let mut content = String::new();
        let is_raw = prefix.contains('r') || prefix.contains('R');
        let is_fstring = prefix.contains('f') || prefix.contains('F');

        let mut triple = false;
        {
            let mut peek_pos = self.pos;
            let ch1 = self.src.get(peek_pos).copied();
            peek_pos += 1;
            let ch2 = self.src.get(peek_pos).copied();
            if ch1 == Some(quote) && ch2 == Some(quote) {
                triple = true;
                self.advance(); self.advance();
            }
        }

        if triple {
            loop {
                match self.advance() {
                    None => break,
                    Some(ch) => {
                        if ch == quote {
                            let n1 = self.peek_char();
                            if n1 == Some(quote) {
                                self.advance();
                                let n2 = self.peek_char();
                                if n2 == Some(quote) {
                                    self.advance();
                                    break;
                                } else {
                                    content.push(quote);
                                    content.push(quote);
                                }
                            } else {
                                content.push(quote);
                            }
                        } else if ch == '\\' && !is_raw {
                            content.push(self.read_escape());
                        } else {
                            content.push(ch);
                        }
                    }
                }
            }
        } else {
            loop {
                match self.advance() {
                    None => break,
                    Some(ch) => {
                        if ch == quote { break; }
                        if ch == '\\' && !is_raw {
                            content.push(self.read_escape());
                        } else {
                            content.push(ch);
                        }
                    }
                }
            }
        }

        let kind = if is_fstring {
            TokenKind::FStringStart
        } else {
            TokenKind::String(content.clone())
        };

        Token {
            kind,
            lexeme: content,
            line: self.line, col: start_col,
            string_id: None, int_value: None, float_value: None,
        }
    }

    fn read_escape(&mut self) -> char {
        match self.advance() {
            Some('n') => '\n',
            Some('t') => '\t',
            Some('r') => '\r',
            Some('\\') => '\\',
            Some('\'') => '\'',
            Some('"') => '"',
            Some('0') => '\0',
            Some('x') => {
                let mut hex = String::new();
                for _ in 0..2 {
                    if let Some(c) = self.peek_char() {
                        if c.is_ascii_hexdigit() { hex.push(c); self.advance(); } else { break; }
                    }
                }
                u8::from_str_radix(&hex, 16).unwrap_or(0) as char
            }
            Some(c) => c,
            None => '\0',
        }
    }

    fn skip_whitespace_on_line(&mut self) {
        while let Some(ch) = self.peek_char() {
            if ch == ' ' || ch == '\t' { self.advance(); }
            else { break; }
        }
    }

    // ── Indentation ──

    fn handle_indent(&mut self, current_indent: usize) -> Option<Token> {
        let last_indent = *self.indent_stack.last().unwrap();
        if current_indent > last_indent {
            self.indent_stack.push(current_indent);
            Some(self.make_token(TokenKind::Indent, ""))
        } else if current_indent < last_indent {
            // Pop only ONE level per call — caller (next) will keep
            // at_line_start=true if more levels remain.
            if self.indent_stack.len() > 1 {
                self.indent_stack.pop();
            }
            Some(self.make_token(TokenKind::Dedent, ""))
        } else {
            None
        }
    }

    fn count_indent(&mut self) -> usize {
        let mut indent = 0;
        let saved_pos = self.pos;
        loop {
            match self.src.get(self.pos) {
                Some(' ') => { indent += 1; self.pos += 1; self.col += 1; }
                Some('\t') => { indent += 4; self.pos += 1; self.col += 1; }
                Some('#') | Some('\n') => {
                    self.pos = saved_pos;
                    return 0;
                }
                _ => break,
            }
        }
        indent
    }

    // ── Public API ──

    pub fn next(&mut self) -> Token {
        if let Some(tok) = self.pending_token.take() {
            return tok;
        }

        // At line start, handle indentation
        if self.at_line_start && self.paren_depth == 0 {
            let saved_pos = self.pos;
            let saved_col = self.col;
            let indent = self.count_indent();
            if let Some(tok) = self.handle_indent(indent) {
                // If we popped a level and more remain, keep at_line_start + restore
                // pos so the next call re-reads the same indent and pops again.
                if matches!(tok.kind, TokenKind::Dedent) && *self.indent_stack.last().unwrap() > indent {
                    self.pos = saved_pos;
                    self.col = saved_col;
                } else {
                    self.at_line_start = false;
                }
                return tok;
            }
            self.at_line_start = false;
        } else {
            self.at_line_start = false;
        }

        // Skip whitespace (but not newlines)
        self.skip_whitespace_on_line();

        // Check for comment
        if self.peek_char() == Some('#') {
            while let Some(ch) = self.peek_char() {
                if ch == '\n' { break; }
                self.advance();
            }
            return self.next();
        }

        // Check for newline
        if let Some('\n') = self.peek_char() {
            self.advance();
            self.at_line_start = true;

            if self.paren_depth == 0 {
                return self.make_token(TokenKind::Newline, "\n");
            }
            self.at_line_start = false;
            return self.next();
        }

        // EOF
        let ch = match self.peek_char() {
            Some(c) => c,
            None => {
                if self.indent_stack.len() > 1 {
                    self.indent_stack.pop();
                    return self.make_token(TokenKind::Dedent, "");
                }
                return self.make_token(TokenKind::EndOfFile, "");
            },
        };

        // String literal
        if ch == '"' || ch == '\'' {
            if self.col > 0 { /* check prefixes handled below */ }
            self.advance();
            return self.read_string(ch, "");
        }

        // Check for string prefix (f, r, b) followed by quote
        if (ch == 'f' || ch == 'F' || ch == 'r' || ch == 'R' || ch == 'b' || ch == 'B')
            && self.pos + 1 < self.src.len()
        {
            let mut prefix = String::new();
            let mut peek_pos = self.pos;
            loop {
                if peek_pos >= self.src.len() { break; }
                let pc = self.src[peek_pos];
                if pc == 'f' || pc == 'F' || pc == 'r' || pc == 'R' || pc == 'b' || pc == 'B' {
                    prefix.push(pc);
                    peek_pos += 1;
                } else {
                    break;
                }
            }
            if peek_pos < self.src.len() {
                let q = self.src[peek_pos];
                if q == '"' || q == '\'' {
                    for _ in 0..prefix.len() { self.advance(); }
                    self.advance(); // consume quote
                    return self.read_string(q, &prefix);
                }
            }
        }

        // Number
        if ch.is_ascii_digit() {
            self.advance();
            return self.read_number(ch);
        }

        // Identifier or keyword
        if ch.is_alphabetic() || ch == '_' {
            return self.read_identifier();
        }

        // Operators and punctuation
        self.advance();
        match ch {
            '+' => {
                if self.peek_char() == Some('=') { self.advance(); self.make_token(TokenKind::PlusEq, "+=") }
                else { self.make_token(TokenKind::Plus, "+") }
            }
            '-' => {
                if self.peek_char() == Some('=') { self.advance(); self.make_token(TokenKind::MinusEq, "-=") }
                else if self.peek_char() == Some('>') { self.advance(); self.make_token(TokenKind::Arrow, "->") }
                else { self.make_token(TokenKind::Minus, "-") }
            }
            '*' => {
                if self.peek_char() == Some('=') { self.advance(); self.make_token(TokenKind::StarEq, "*=") }
                else if self.peek_char() == Some('*') { self.advance();
                    if self.peek_char() == Some('=') { self.advance(); self.make_token(TokenKind::DoubleStarEq, "**=") }
                    else { self.make_token(TokenKind::DoubleStar, "**") }
                }
                else { self.make_token(TokenKind::Star, "*") }
            }
            '/' => {
                if self.peek_char() == Some('=') { self.advance(); self.make_token(TokenKind::SlashEq, "/=") }
                else if self.peek_char() == Some('/') { self.advance(); self.make_token(TokenKind::FloorDiv, "//") }
                else { self.make_token(TokenKind::Slash, "/") }
            }
            '%' => {
                if self.peek_char() == Some('=') { self.advance(); self.make_token(TokenKind::PercentEq, "%=") }
                else { self.make_token(TokenKind::Percent, "%") }
            }
            '=' => {
                if self.peek_char() == Some('=') { self.advance(); self.make_token(TokenKind::EqEq, "==") }
                else { self.make_token(TokenKind::Eq, "=") }
            }
            '!' => {
                if self.peek_char() == Some('=') { self.advance(); self.make_token(TokenKind::NotEq, "!=") }
                else { self.make_token(TokenKind::Not, "!") }
            }
            '<' => {
                if self.peek_char() == Some('=') { self.advance(); self.make_token(TokenKind::LtE, "<=") }
                else if self.peek_char() == Some('<') { self.advance(); self.make_token(TokenKind::LShift, "<<") }
                else { self.make_token(TokenKind::Lt, "<") }
            }
            '>' => {
                if self.peek_char() == Some('=') { self.advance(); self.make_token(TokenKind::GtE, ">=") }
                else if self.peek_char() == Some('>') { self.advance(); self.make_token(TokenKind::RShift, ">>") }
                else { self.make_token(TokenKind::Gt, ">") }
            }
            '&' => { self.make_token(TokenKind::BitAnd, "&") }
            '|' => { self.make_token(TokenKind::BitOr, "|") }
            '^' => { self.make_token(TokenKind::BitXor, "^") }
            '~' => { self.make_token(TokenKind::BitNot, "~") }
            '@' => { self.make_token(TokenKind::At, "@") }
            '.' => {
                if self.peek_char() == Some('.') { self.advance();
                    if self.peek_char() == Some('.') { self.advance(); self.make_token(TokenKind::Ellipsis, "...") }
                    else { self.make_token(TokenKind::Dot, "..") }
                }
                else { self.make_token(TokenKind::Dot, ".") }
            }
            ':' => {
                if self.peek_char() == Some('=') { self.advance(); self.make_token(TokenKind::Walrus, ":=") }
                else { self.make_token(TokenKind::Colon, ":") }
            }
            '(' => { self.paren_depth += 1; self.make_token(TokenKind::LParen, "(") }
            ')' => { if self.paren_depth > 0 { self.paren_depth -= 1; } self.make_token(TokenKind::RParen, ")") }
            '[' => { self.paren_depth += 1; self.make_token(TokenKind::LBracket, "[") }
            ']' => { if self.paren_depth > 0 { self.paren_depth -= 1; } self.make_token(TokenKind::RBracket, "]") }
            '{' => { self.paren_depth += 1; self.make_token(TokenKind::LBrace, "{") }
            '}' => { if self.paren_depth > 0 { self.paren_depth -= 1; } self.make_token(TokenKind::RBrace, "}") }
            ',' => self.make_token(TokenKind::Comma, ","),
            ';' => self.make_token(TokenKind::Semicolon, ";"),
            _ => self.make_token(TokenKind::Name(format!("{ch}")), &ch.to_string()),
        }
    }
}

// ── Keyword table ──

fn keyword(s: &str) -> Option<TokenKind> {
    Some(match s {
        "def" => TokenKind::KwDef,
        "if" => TokenKind::KwIf,
        "elif" => TokenKind::KwElif,
        "else" => TokenKind::KwElse,
        "while" => TokenKind::KwWhile,
        "for" => TokenKind::KwFor,
        "in" => TokenKind::KwIn,
        "return" => TokenKind::KwReturn,
        "break" => TokenKind::KwBreak,
        "continue" => TokenKind::KwContinue,
        "pass" => TokenKind::KwPass,
        "import" => TokenKind::KwImport,
        "from" => TokenKind::KwFrom,
        "as" => TokenKind::KwAs,
        "class" => TokenKind::KwClass,
        "try" => TokenKind::KwTry,
        "except" => TokenKind::KwExcept,
        "finally" => TokenKind::KwFinally,
        "raise" => TokenKind::KwRaise,
        "with" => TokenKind::KwWith,
        "yield" => TokenKind::KwYield,
        "lambda" => TokenKind::KwLambda,
        "and" => TokenKind::KwAnd,
        "or" => TokenKind::KwOr,
        "not" => TokenKind::KwNot,
        "is" => TokenKind::KwIs,
        "None" => TokenKind::KwNone,
        "True" => TokenKind::KwTrue,
        "False" => TokenKind::KwFalse,
        "global" => TokenKind::KwGlobal,
        "nonlocal" => TokenKind::KwNonlocal,
        "del" => TokenKind::KwDel,
        "assert" => TokenKind::KwAssert,
        "async" => TokenKind::KwAsync,
        "await" => TokenKind::KwAwait,
        "match" => TokenKind::KwMatch,
        "case" => TokenKind::KwCase,
        _ => return None,
    })
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.kind)
    }
}
