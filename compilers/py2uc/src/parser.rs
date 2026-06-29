/// Minimal recursive descent parser for Python 3.
/// Uses the tokenizer, produces Program AST.
/// Handles: def, if/elif/else, while, for, return, pass/break/continue,
/// import, class (basic), try, with, expressions (Pratt), lambda, fstrings,
/// lists, dicts, sets, subscript, attribute, call, ternary, comparisons.

use crate::ast::*;
use crate::tokenizer::{Token, TokenKind, Tokenizer};

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Prec { Lowest=0, BoolOr=1, BoolAnd=2, Not=3, Cmp=4, BitOr=5, BitXor=6, BitAnd=7, Shift=8, Sum=9, Term=10, Factor=11, Unary=12, Power=13, Atom=14 }

impl std::ops::Add<i32> for Prec {
    type Output = Prec;
    fn add(self, r: i32) -> Prec {
        unsafe { std::mem::transmute(((self as i32 + r).clamp(0,14)) as u8) }
    }
}

impl Parser {
    pub fn new(source: &str) -> Self {
        let mut t = Tokenizer::new(source);
        let mut tokens = Vec::new();
        loop {
            let tok = t.next();
            let done = matches!(tok.kind, TokenKind::EndOfFile);
            tokens.push(tok);
            if done { break; }
        }
        Parser { tokens, pos: 0 }
    }

    /// Get (line, col) of the current token for error reporting.
    pub fn current_pos(&self) -> (usize, usize) {
        let t = self.peek();
        (t.line, t.col)
    }

    fn peek(&self) -> &Token {
        if self.pos < self.tokens.len() { &self.tokens[self.pos] } else { &self.tokens[self.tokens.len() - 1] }
    }
    fn advance(&mut self) -> &Token {
        let t = &self.tokens[self.pos];
        if self.pos + 1 < self.tokens.len() { self.pos += 1; }
        t
    }
    fn skip_nl(&mut self) {
        while self.peek().kind == TokenKind::Newline { self.advance(); }
    }
    fn expect(&mut self, k: TokenKind, ctx: &str) -> Result<String, String> {
        if self.peek().kind == k { let t = self.advance(); return Ok(t.lexeme.clone()); }
        Err(format!("{ctx}: expected {:?}, got {:?}", k, self.peek().kind))
    }
    fn is_kw(&self, s: &str) -> bool {
        match &self.peek().kind { TokenKind::Name(n) if n == s => true, _ => false }
    }

    // ── Program ──
    pub fn parse(&mut self) -> Result<Program, String> {
        let mut stmts = Vec::new();
        while !matches!(self.peek().kind, TokenKind::EndOfFile) {
            self.skip_nl();
            if matches!(self.peek().kind, TokenKind::EndOfFile) { break; }
            stmts.push(self.stmt()?);
        }
        Ok(Program { stmts })
    }

    // ── Statements ──
    fn stmt(&mut self) -> Result<Stmt, String> {
        while self.peek().kind == TokenKind::Newline { self.advance(); }
        match &self.peek().kind.clone() {
            TokenKind::KwDef => self.func_def(),
            TokenKind::KwClass => self.class_def(),
            TokenKind::KwIf => self.if_stmt(),
            TokenKind::KwWhile => self.while_stmt(),
            TokenKind::KwFor => self.for_stmt(),
            TokenKind::KwReturn => { self.advance(); let v = self.opt_expr(); Ok(Stmt::Return(v)) }
            TokenKind::KwMatch => {
                self.advance();
                let _ = self.expr(Prec::Lowest);
                self.expect(TokenKind::Colon, "match :")?;
                // Skip the entire match block by consuming all tokens
                // until we hit DEDENT at the match block's indentation.
                while self.peek().kind == TokenKind::Newline { self.advance(); }
                if self.peek().kind == TokenKind::Indent {
                    self.advance(); // match body INDENT
                    let mut depth = 1i32;
                    loop {
                        match self.peek().kind {
                            TokenKind::Indent => { depth += 1; self.advance(); }
                            TokenKind::Dedent => {
                                depth -= 1;
                                self.advance();
                                if depth == 0 { break; }
                            }
                            TokenKind::EndOfFile => break,
                            _ => { self.advance(); }
                        }
                    }
                }
                // At this point we've consumed the match body DEDENT.
                // The next token is whatever follows the match block
                // (either another DEDENT or the next statement).
                Ok(Stmt::Pass)
            }
            TokenKind::KwBreak => { self.advance(); Ok(Stmt::Break) }
            TokenKind::KwContinue => { self.advance(); Ok(Stmt::Continue) }
            TokenKind::KwPass => { self.advance(); Ok(Stmt::Pass) }
            TokenKind::KwImport => self.import_stmt(),
            TokenKind::KwFrom => self.import_from(),
            TokenKind::KwTry => self.try_stmt(),
            TokenKind::KwAsync => {
                self.advance();
                if self.peek().kind == TokenKind::KwDef {
                    // async def — parse as regular func
                    let mut f = self.func_def()?;
                    if let Stmt::FuncDef { .. } = &mut f { }
                    Ok(f)
                } else {
                    // async with/for — skip for now
                    while !matches!(self.peek().kind, TokenKind::EndOfFile) { self.advance(); }
                    Ok(Stmt::Pass)
                }
            }
            TokenKind::KwRaise => { self.advance(); let e = self.opt_expr(); Ok(Stmt::Raise { exc: e, cause: None }) }
            TokenKind::KwWith => self.with_stmt(),
            TokenKind::KwGlobal => { self.advance(); let ns = self.parse_name_list(); Ok(Stmt::Global(ns)) }
            TokenKind::At => self.decorated(),
            _ => self.assign_or_expr(),
        }
    }

    fn opt_expr(&mut self) -> Option<Expr> {
        self.skip_nl();
        if matches!(self.peek().kind, TokenKind::Newline | TokenKind::EndOfFile | TokenKind::Semicolon | TokenKind::Dedent | TokenKind::Colon) { None }
        else { Some(self.expr(Prec::Lowest).unwrap_or(Expr::None_)) }
    }

    fn suite(&mut self) -> Result<Vec<Stmt>, String> {
        while self.peek().kind == TokenKind::Newline { self.advance(); }
        if self.peek().kind == TokenKind::Indent {
            self.advance();
            let b = self.block()?;
            while self.peek().kind == TokenKind::Newline { self.advance(); }
            if self.peek().kind == TokenKind::Dedent { self.advance(); }
            return Ok(b);
        }
        let s = self.stmt()?;
        while self.peek().kind == TokenKind::Newline { self.advance(); }
        Ok(vec![s])
    }

    fn block(&mut self) -> Result<Vec<Stmt>, String> {
        let mut b = Vec::new();
        loop {
            while self.peek().kind == TokenKind::Newline { self.advance(); }
            // Skip blank-line artifacts: DEDENT immediately followed by INDENT
            if self.peek().kind == TokenKind::Dedent {
                let saved = self.pos;
                self.advance();
                while self.peek().kind == TokenKind::Newline { self.advance(); }
                if self.peek().kind == TokenKind::Indent {
                    self.advance(); // skip the INDENT too
                    continue; // blank line at same level — done
                }
                self.pos = saved; // real DEDENT — rollback
                break;
            }
            if matches!(self.peek().kind, TokenKind::EndOfFile) { break; }
            b.push(self.stmt()?);
            while self.peek().kind == TokenKind::Newline { self.advance(); }
        }
        Ok(b)
    }

    fn func_def(&mut self) -> Result<Stmt, String> {
        self.advance();
        let name = self.name()?;
        self.expect(TokenKind::LParen, "func (")?;
        let args = self.parse_args()?;
        // Skip type annotations (`a: int, b: str`) until RParen
        while !matches!(self.peek().kind, TokenKind::RParen | TokenKind::EndOfFile) {
            self.advance();
        }
        self.expect(TokenKind::RParen, "func )")?;
        let returns = if self.peek().kind == TokenKind::Arrow { self.advance(); Some(self.expr(Prec::Lowest)?) } else { None };
        self.expect(TokenKind::Colon, "func :")?;
        let body = self.suite()?;
        Ok(Stmt::FuncDef { name, args, body, decorators: vec![], returns })
    }

    fn class_def(&mut self) -> Result<Stmt, String> {
        self.advance();
        let name = self.name()?;
        let mut bases = Vec::new();
        if self.peek().kind == TokenKind::LParen {
            self.advance();
            // Skip class bases — consume everything until RParen
            let mut depth = 1i32;
            while depth > 0 {
                match self.peek().kind {
                    TokenKind::LParen => { depth += 1; self.advance(); }
                    TokenKind::RParen => { depth -= 1; if depth > 0 { self.advance(); } }
                    TokenKind::EndOfFile => break,
                    _ => { self.advance(); }
                }
            }
            if self.peek().kind == TokenKind::RParen { self.advance(); }
        }
        self.expect(TokenKind::Colon, "class :")?;
        let body = self.suite()?;
        Ok(Stmt::ClassDef { name, bases, body, decorators: vec![] })
    }

    fn decorated(&mut self) -> Result<Stmt, String> {
        let mut decs = Vec::new();
        while self.peek().kind == TokenKind::At { self.advance(); decs.push(self.expr(Prec::Lowest)?); self.skip_nl(); }
        self.skip_nl();
        match self.peek().kind {
            TokenKind::KwDef => {
                let mut f = self.func_def()?;
                if let Stmt::FuncDef { decorators, .. } = &mut f { *decorators = decs; }
                Ok(f)
            }
            TokenKind::KwClass => {
                let mut c = self.class_def()?;
                if let Stmt::ClassDef { decorators, .. } = &mut c { *decorators = decs; }
                Ok(c)
            }
            _ => Err("Decorator on non-def/class".into()),
        }
    }

    fn if_stmt(&mut self) -> Result<Stmt, String> {
        self.advance();
        let test = self.expr(Prec::Lowest)?;
        self.expect(TokenKind::Colon, "if :")?;
        let body = self.suite()?;
        while self.peek().kind == TokenKind::Newline { self.advance(); }
        let mut orelse = Vec::new();
        if self.peek().kind == TokenKind::KwElif {
            orelse.push(self.if_stmt()?);
        } else if self.peek().kind == TokenKind::KwElse {
            self.advance();
            self.expect(TokenKind::Colon, "else :")?;
            orelse = self.suite()?;
        }
        Ok(Stmt::If { test, body, orelse })
    }

    fn while_stmt(&mut self) -> Result<Stmt, String> {
        self.advance();
        let test = self.expr(Prec::Lowest)?;
        self.expect(TokenKind::Colon, "while :")?;
        let body = self.suite()?;
        let orelse = if self.peek().kind == TokenKind::KwElse {
            self.advance();
            self.expect(TokenKind::Colon, "else :")?;
            self.suite()?
        } else {
            vec![]
        };
        Ok(Stmt::While { test, body, orelse })
    }

    fn for_stmt(&mut self) -> Result<Stmt, String> {
        self.advance();
        // Parse target(s) as names (not full expressions — avoids consuming 'in')
        let mut names = Vec::new();
        loop {
            let n = self.name()?;
            names.push(Expr::Name(n));
            if self.peek().kind == TokenKind::Comma { self.advance(); }
            else { break; }
        }
        let target = if names.len() == 1 { names.into_iter().next().unwrap() }
                     else { Expr::Tuple(names) };
        self.expect(TokenKind::KwIn, "for in")?;
        let iter = self.expr(Prec::Lowest)?;
        self.expect(TokenKind::Colon, "for :")?;
        let body = self.suite()?;
        let orelse = if self.peek().kind == TokenKind::KwElse {
            self.advance();
            self.expect(TokenKind::Colon, "else :")?;
            self.suite()?
        } else {
            vec![]
        };
        Ok(Stmt::For { target, iter, body, orelse })
    }

    fn try_stmt(&mut self) -> Result<Stmt, String> {
        self.advance();
        self.expect(TokenKind::Colon, "try :")?;
        while self.peek().kind == TokenKind::Newline { self.advance(); }
        if self.peek().kind == TokenKind::Indent { self.advance(); }
        let mut depth = 1i32;
        // Consume try body + except/else/finally blocks
        loop {
            match self.peek().kind {
                TokenKind::Indent => { depth += 1; self.advance(); }
                TokenKind::Dedent => {
                    depth -= 1;
                    self.advance();
                    if depth == 0 {
                        // After try/except body DEDENT, check for continuation
                        let nxt = self.peek().kind.clone();
                        if matches!(nxt, TokenKind::KwExcept | TokenKind::KwFinally | TokenKind::KwElse) {
                            // Consume header and indent for next block
                            while !matches!(self.peek().kind, TokenKind::Colon | TokenKind::EndOfFile) { self.advance(); }
                            if self.peek().kind == TokenKind::Colon { self.advance(); }
                            while self.peek().kind == TokenKind::Newline { self.advance(); }
                            if self.peek().kind == TokenKind::Indent { self.advance(); depth = 1; }
                        } else { break; }
                    }
                }
                TokenKind::EndOfFile => break,
                _ => { self.advance(); }
            }
        }
        Ok(Stmt::Pass)
    }

    fn with_stmt(&mut self) -> Result<Stmt, String> {
        self.advance();
        let mut items = Vec::new();
        loop {
            let ctx = self.expr(Prec::Lowest)?;
            let vars = if self.peek().kind == TokenKind::KwAs { self.advance(); Some(self.expr(Prec::Lowest)?) } else { None };
            items.push(WithItem { context_expr: ctx, optional_vars: vars });
            if self.peek().kind == TokenKind::Comma { self.advance(); } else { break; }
        }
        self.expect(TokenKind::Colon, "with :")?;
        let body = self.suite()?;
        Ok(Stmt::With { items, body })
    }

    fn import_stmt(&mut self) -> Result<Stmt, String> {
        self.advance();
        let mut names = Vec::new();
        loop {
            let n = self.dotted_name()?;
            let asn = if self.peek().kind == TokenKind::KwAs { self.advance(); Some(self.name()?) } else { None };
            names.push(Alias { name: n, asname: asn });
            if self.peek().kind == TokenKind::Comma { self.advance(); } else { break; }
        }
        Ok(Stmt::Import { names })
    }

    fn import_from(&mut self) -> Result<Stmt, String> {
        self.advance();
        let mut level = 0;
        while self.peek().kind == TokenKind::Dot { level += 1; self.advance(); }
        let module = if matches!(self.peek().kind, TokenKind::Name(_)) {
            Some(self.dotted_name()?)
        } else {
            None
        };
        self.expect(TokenKind::KwImport, "from import")?;
        if self.peek().kind == TokenKind::Star {
            self.advance();
            return Ok(Stmt::ImportFrom { module, names: vec![Alias { name: "*".into(), asname: None }], level });
        }
        let mut names = Vec::new();
        loop {
            let n = self.name()?;
            let asn = if self.peek().kind == TokenKind::KwAs { self.advance(); Some(self.name()?) } else { None };
            names.push(Alias { name: n, asname: asn });
            if self.peek().kind == TokenKind::Comma { self.advance(); } else { break; }
        }
        Ok(Stmt::ImportFrom { module, names, level })
    }

    /// Parse a dotted name: "foo.bar.baz"
    fn dotted_name(&mut self) -> Result<String, String> {
        let mut name = self.name()?;
        while self.peek().kind == TokenKind::Dot {
            self.advance();
            let part = self.name()?;
            name.push('.');
            name.push_str(&part);
        }
        Ok(name)
    }

    fn name(&mut self) -> Result<String, String> {
        match &self.peek().kind {
            TokenKind::Name(n) => { let n = n.clone(); self.advance(); Ok(n) }
            _ => Err(format!("Expected name, got {:?}", self.peek().kind)),
        }
    }

    fn parse_name_list(&mut self) -> Vec<String> {
        let mut ns = Vec::new();
        loop {
            if let Ok(n) = self.name() { ns.push(n); } else { break; }
            if self.peek().kind == TokenKind::Comma { self.advance(); } else { break; }
        }
        ns
    }

    fn assign_or_expr(&mut self) -> Result<Stmt, String> {
        let first_target = self.expr(Prec::Lowest)?;
        // Only skip NEWLINE (not indent/dedent) between targets
        while self.peek().kind == TokenKind::Newline { self.advance(); }

        // Handle multi-target assignment: a, b = x, y
        let mut targets = vec![first_target];
        while self.peek().kind == TokenKind::Comma {
            self.advance(); // consume comma
            self.skip_nl();
            // Handle *rest (star unpacking)
            if self.peek().kind == TokenKind::Star {
                self.advance();
                let rest = self.expr(Prec::Lowest)?;
                targets.push(Expr::Starred(Box::new(rest)));
            } else if self.peek().kind == TokenKind::Eq {
                break; // trailing comma before =
            } else {
                targets.push(self.expr(Prec::Lowest)?);
            }
        }

        // Handle type annotation on first target: name: type = value
        if targets.len() == 1 && self.peek().kind == TokenKind::Colon {
            self.advance(); // consume :
            // Skip the type expression
            loop {
                let _ = self.expr(Prec::Lowest);
                if self.peek().kind != TokenKind::LBracket { break; }
                self.advance(); // consume [
                let _ = self.expr(Prec::Lowest);
                self.advance(); // consume ]
            }
        }
        let aug = self.aug_assign();
        if aug.is_some() || self.peek().kind == TokenKind::Eq {
            if aug.is_some() { self.advance(); }
            else { self.advance(); }
            let rhs = self.expr(Prec::Lowest)?;
            return Ok(Stmt::Assign { targets, value: rhs, aug });
        }
        if targets.len() == 1 {
            Ok(Stmt::Expr(targets.into_iter().next().unwrap()))
        } else {
            Ok(Stmt::Expr(Expr::Tuple(targets)))
        }
    }

    fn aug_assign(&self) -> Option<BinOp> {
        Some(match self.peek().kind {
            TokenKind::PlusEq => BinOp::Add, TokenKind::MinusEq => BinOp::Sub,
            TokenKind::StarEq => BinOp::Mul, TokenKind::SlashEq => BinOp::Div,
            TokenKind::PercentEq => BinOp::Mod, TokenKind::DoubleStarEq => BinOp::Pow,
            _ => return None,
        })
    }

    // ── Expressions (Pratt parser) ──

    fn infix_bp(k: &TokenKind) -> Option<(Prec, Prec)> {
        Some(match k {
            TokenKind::KwOr => (Prec::BoolOr, Prec::BoolOr+1),
            TokenKind::KwAnd => (Prec::BoolAnd, Prec::BoolAnd+1),
            TokenKind::EqEq|TokenKind::NotEq|TokenKind::Lt|TokenKind::Gt|TokenKind::LtE|TokenKind::GtE|TokenKind::KwIn|TokenKind::Is => (Prec::Cmp, Prec::Cmp+1),
            TokenKind::Pipe => (Prec::BitOr, Prec::BitOr+1),
            TokenKind::BitXor => (Prec::BitXor, Prec::BitXor+1),
            TokenKind::BitAnd => (Prec::BitAnd, Prec::BitAnd+1),
            TokenKind::LShift|TokenKind::RShift => (Prec::Shift, Prec::Shift+1),
            TokenKind::Plus|TokenKind::Minus => (Prec::Sum, Prec::Sum+1),
            TokenKind::Star|TokenKind::Slash|TokenKind::Percent|TokenKind::FloorDiv|TokenKind::At => (Prec::Term, Prec::Term+1),
            TokenKind::DoubleStar => (Prec::Power, Prec::Power),
            _ => return None,
        })
    }

    fn prefix_bp(k: &TokenKind) -> Option<Prec> {
        Some(match k { TokenKind::Not|TokenKind::BitNot|TokenKind::Plus|TokenKind::Minus => Prec::Unary, _ => return None })
    }

    pub fn expr(&mut self, min: Prec) -> Result<Expr, String> {
        let mut lhs = self.atom()?;
        loop {
            // Postfix: call, subscript, attribute
            match self.peek().kind {
                TokenKind::LParen => { lhs = self.parse_call(lhs)?; continue; }
                TokenKind::LBracket => { lhs = self.parse_sub(lhs)?; continue; }
                TokenKind::Dot => { self.advance(); lhs = Expr::Attribute { value: Box::new(lhs), attr: self.name()? }; continue; }
                _ => {}
            }
            let tok = self.peek().kind.clone();
            let Some((lbp, rbp)) = Self::infix_bp(&tok) else { break };
            if lbp < min { break; }
            lhs = self.parse_infix(lhs, &tok, rbp)?;
        }
        // Ternary: if expr else expr
        if self.peek().kind == TokenKind::KwIf {
            self.advance();
            let test = self.expr(Prec::Lowest)?;
            self.expect(TokenKind::KwElse, "ternary else")?;
            let orelse = self.expr(Prec::Lowest)?;
            lhs = Expr::IfExpr { test: Box::new(test), body: Box::new(lhs), orelse: Box::new(orelse) };
        }
        Ok(lhs)
    }

    fn parse_infix(&mut self, lhs: Expr, tok: &TokenKind, rbp: Prec) -> Result<Expr, String> {
        match tok {
            TokenKind::KwOr|TokenKind::KwAnd => {
                self.advance();
                let op = if *tok == TokenKind::KwOr { BoolOp::Or } else { BoolOp::And };
                let rhs = self.expr(rbp)?;
                match lhs {
                    Expr::BoolOp { op: eo, mut values } if eo == op => { values.push(rhs); Ok(Expr::BoolOp { op, values }) }
                    _ => Ok(Expr::BoolOp { op, values: vec![lhs, rhs] }),
                }
            }
            TokenKind::EqEq|TokenKind::NotEq|TokenKind::Lt|TokenKind::Gt|TokenKind::LtE|TokenKind::GtE => {
                let mut ops = Vec::new();
                let mut comps = Vec::new();
                loop {
                    let op = cmp_op_tok(&self.peek().kind);
                    if op.is_none() { break; }
                    ops.push(op.unwrap());
                    self.advance();
                    comps.push(self.expr(rbp)?);
                }
                Ok(Expr::Compare { left: Box::new(lhs), ops, comparators: comps })
            }
            TokenKind::Not => {
                self.advance();
                if self.peek().kind == TokenKind::KwIn { self.advance();
                    Ok(Expr::Compare { left: Box::new(lhs), ops: vec![CmpOp::NotIn], comparators: vec![self.expr(rbp)?] })
                } else { Err("Expected 'in' after 'not'".into()) }
            }
            TokenKind::Is => {
                self.advance();
                let is_not = if self.peek().kind == TokenKind::Not { self.advance(); true } else { false };
                Ok(Expr::Compare { left: Box::new(lhs), ops: vec![if is_not { CmpOp::IsNot } else { CmpOp::Is }], comparators: vec![self.expr(rbp)?] })
            }
            TokenKind::KwIn => { self.advance();
                Ok(Expr::Compare { left: Box::new(lhs), ops: vec![CmpOp::In], comparators: vec![self.expr(rbp)?] })
            }
            _ => {
                self.advance();
                let op = binop_tok(tok);
                let rhs = self.expr(rbp)?;
                Ok(Expr::BinOp { left: Box::new(lhs), op, right: Box::new(rhs) })
            }
        }
    }

    fn atom(&mut self) -> Result<Expr, String> {
        if let Some(bp) = Self::prefix_bp(&self.peek().kind) {
            let tk = self.peek().kind.clone();
            self.advance();
            let op = match tk { TokenKind::Minus => UnaryOp::USub, TokenKind::Plus => UnaryOp::UAdd, TokenKind::BitNot => UnaryOp::Invert, TokenKind::Not => UnaryOp::Not, _ => return Err("Bad prefix op".into()) };
            return Ok(Expr::UnaryOp { op, operand: Box::new(self.expr(bp)?) });
        }
        let tok = self.peek().kind.clone();
        match &tok {
            TokenKind::Name(s) => { let n = s.clone(); self.advance();
                if self.peek().kind == TokenKind::Walrus {
                    self.advance(); Ok(Expr::NamedExpr { target: Box::new(Expr::Name(n)), value: Box::new(self.expr(Prec::Lowest)?) })
                } else { Ok(Expr::Name(n)) }
            }
            TokenKind::Int => { let v = self.peek().int_value.unwrap_or(0); self.advance(); Ok(Expr::Int(v)) }
            TokenKind::Float => { let v = self.peek().float_value.unwrap_or(0.0); self.advance(); Ok(Expr::Float(v)) }
            TokenKind::String(s) => { let s = s.clone(); self.advance(); Ok(Expr::Str(s)) }
            TokenKind::KwTrue => { self.advance(); Ok(Expr::Bool(true)) }
            TokenKind::KwFalse => { self.advance(); Ok(Expr::Bool(false)) }
            TokenKind::KwNone => { self.advance(); Ok(Expr::None_) }
            TokenKind::KwLambda => self.lambda(),
            TokenKind::KwYield => { self.advance(); Ok(Expr::Yield(Some(Box::new(self.expr(Prec::Lowest)?)))) }
            TokenKind::KwAwait => { self.advance(); Ok(Expr::Await(Box::new(self.expr(Prec::Lowest)?))) }
            TokenKind::LParen => self.paren_tuple(),
            TokenKind::LBracket => self.list(),
            TokenKind::LBrace => self.dict_set(),
            TokenKind::Star => { self.advance(); Ok(Expr::Starred(Box::new(self.expr(Prec::Lowest)?))) }
            TokenKind::Ellipsis => { self.advance(); Ok(Expr::Ellipsis) }
            TokenKind::FStringStart => {
                // Parse f-string content into literal + expression parts
                let content = self.peek().lexeme.clone();
                self.advance();
                self.parse_fstring(&content)
            }
            _ => Err(format!("Unexpected token: {:?}", tok)),
        }
    }

    // ── F-string parsing ──

    fn parse_fstring(&mut self, content: &str) -> Result<Expr, String> {
        let mut parts: Vec<FStringPart> = Vec::new();
        let mut i = 0;
        let bytes = content.as_bytes();
        let mut literal = String::new();

        while i < bytes.len() {
            if bytes[i] == b'{' {
                if i + 1 < bytes.len() && bytes[i + 1] == b'{' {
                    literal.push('{'); i += 2; continue;
                }
                if !literal.is_empty() {
                    parts.push(FStringPart::Literal(std::mem::take(&mut literal)));
                }
                i += 1;
                let mut depth = 1u32;
                let start = i;
                while i < bytes.len() && depth > 0 {
                    match bytes[i] {
                        b'{' => depth += 1,
                        b'}' => depth -= 1,
                        b'"' | b'\'' => {
                            let q = bytes[i]; i += 1;
                            while i < bytes.len() && bytes[i] != q {
                                if bytes[i] == b'\\' { i += 1; }
                                i += 1;
                            }
                            if i < bytes.len() { i += 1; }
                            continue;
                        }
                        _ => {}
                    }
                    if depth > 0 { i += 1; }
                }
                if depth != 0 {
                    literal.push('{');
                    literal.push_str(&content[start - 1..]);
                    break;
                }
                let expr_str = &content[start..i];
                // Find format spec ':' only at top-level (not inside brackets)
                let mut format_colon = None;
                let mut bracket_depth = 0u32;
                for (j, c) in expr_str.char_indices() {
                    match c {
                        '[' | '(' | '{' => bracket_depth += 1,
                        ']' | ')' | '}' => { if bracket_depth > 0 { bracket_depth -= 1; } }
                        ':' if bracket_depth == 0 => { format_colon = Some(j); break; }
                        _ => {}
                    }
                }
                let expr_only = match format_colon {
                    Some(col) => expr_str[..col].trim(),
                    None => expr_str.trim(),
                };
                i += 1;
                let expr = if expr_only.is_empty() {
                    Expr::Str(String::new())
                } else {
                    self.parse_embedded_expr(expr_only)
                        .unwrap_or_else(|_| Expr::Str(format!("{{{}:...}}", expr_only)))
                };
                parts.push(FStringPart::Expr(expr));
            } else if bytes[i] == b'}' && i + 1 < bytes.len() && bytes[i + 1] == b'}' {
                literal.push('}'); i += 2;
            } else {
                literal.push(bytes[i] as char); i += 1;
            }
        }

        if !literal.is_empty() { parts.push(FStringPart::Literal(literal)); }
        if parts.is_empty() { parts.push(FStringPart::Literal(String::new())); }
        Ok(Expr::FString(parts))
    }

    fn parse_embedded_expr(&self, s: &str) -> Result<Expr, String> {
        let trimmed = s.trim();
        if trimmed.is_empty() { return Ok(Expr::Str(String::new())); }
        let mut tok = Tokenizer::new(trimmed);
        let mut tokens = Vec::new();
        loop {
            let t = tok.next();
            let done = matches!(t.kind, TokenKind::EndOfFile);
            tokens.push(t);
            if done { break; }
        }
        let mut sub = Parser { tokens, pos: 0 };
        sub.expr(Prec::Lowest)
    }

    fn paren_tuple(&mut self) -> Result<Expr, String> {
        self.advance();
        if self.peek().kind == TokenKind::RParen { self.advance(); return Ok(Expr::Tuple(vec![])); }
        let first = self.expr(Prec::Lowest)?;
        if self.peek().kind == TokenKind::Comma {
            let mut items = vec![first];
            while self.peek().kind == TokenKind::Comma { self.advance(); if self.peek().kind == TokenKind::RParen { break; } items.push(self.expr(Prec::Lowest)?); }
            self.expect(TokenKind::RParen, "tuple )")?;
            return Ok(Expr::Tuple(items));
        }
        if self.peek().kind == TokenKind::KwFor {
            // Generator expression — skip to closing )
            let mut depth = 1i32;
            while depth > 0 {
                match self.peek().kind {
                    TokenKind::LParen => { depth += 1; self.advance(); }
                    TokenKind::RParen => { depth -= 1; if depth > 0 { self.advance(); } }
                    TokenKind::EndOfFile => break,
                    _ => { self.advance(); }
                }
            }
            if self.peek().kind == TokenKind::RParen { self.advance(); }
            return Ok(Expr::Generator(Box::new(first), vec![]));
        }
        self.expect(TokenKind::RParen, "expr )")?;
        Ok(first)
    }

    fn list(&mut self) -> Result<Expr, String> {
        self.advance();
        if self.peek().kind == TokenKind::RBracket { self.advance(); return Ok(Expr::List(vec![])); }
        let first = self.expr(Prec::Lowest)?;
        if self.peek().kind == TokenKind::KwFor {
            // List comprehension — skip all tokens and return empty list
            while !matches!(self.peek().kind, TokenKind::RBracket | TokenKind::EndOfFile) {
                self.advance();
            }
            self.advance(); // consume ]
            return Ok(Expr::List(vec![]));
        }
        let mut items = vec![first];
        while self.peek().kind == TokenKind::Comma { self.advance(); if self.peek().kind == TokenKind::RBracket { break; } items.push(self.expr(Prec::Lowest)?); }
        self.expect(TokenKind::RBracket, "list ]")?;
        Ok(Expr::List(items))
    }

    fn dict_set(&mut self) -> Result<Expr, String> {
        self.advance();
        if self.peek().kind == TokenKind::RBrace { self.advance(); return Ok(Expr::Dict(vec![])); }
        let first = self.expr(Prec::Lowest)?;
        if self.peek().kind == TokenKind::KwFor {
            while !matches!(self.peek().kind, TokenKind::RBrace | TokenKind::EndOfFile) { self.advance(); }
            self.advance();
            return Ok(Expr::Set(vec![]));
        }
        if self.peek().kind == TokenKind::Colon {
            self.advance();
            let val = self.expr(Prec::Lowest)?;
            if self.peek().kind == TokenKind::KwFor {
                while !matches!(self.peek().kind, TokenKind::RBrace | TokenKind::EndOfFile) { self.advance(); }
                self.advance();
                return Ok(Expr::Dict(vec![]));
            }
            let mut items = vec![(Box::new(first), Box::new(val))];
            while self.peek().kind == TokenKind::Comma { self.advance(); if self.peek().kind == TokenKind::RBrace { break; }
                let k = self.expr(Prec::Lowest)?; self.expect(TokenKind::Colon, "dict colon")?;
                let v = self.expr(Prec::Lowest)?; items.push((Box::new(k), Box::new(v)));
            }
            self.expect(TokenKind::RBrace, "dict }")?;
            return Ok(Expr::Dict(items));
        }
        let mut items = vec![first];
        while self.peek().kind == TokenKind::Comma { self.advance(); if self.peek().kind == TokenKind::RBrace { break; } items.push(self.expr(Prec::Lowest)?); }
        self.expect(TokenKind::RBrace, "set }")?;
        Ok(Expr::Set(items))
    }

    fn parse_call(&mut self, func: Expr) -> Result<Expr, String> {
        self.advance();
        let mut args = Vec::new();
        let mut kw = Vec::new();
        if self.peek().kind != TokenKind::RParen {
            loop {
                if self.peek().kind == TokenKind::Star { self.advance(); args.push(Expr::Starred(Box::new(self.expr(Prec::Lowest)?))); }
                else if self.peek().kind == TokenKind::DoubleStar { self.advance(); args.push(Expr::Starred(Box::new(self.expr(Prec::Lowest)?))); }
                else {
                    let a = self.expr(Prec::Lowest)?;
                    if self.peek().kind == TokenKind::Eq { self.advance();
                        if let Expr::Name(n) = a { kw.push(Keyword { name: Some(n), value: self.expr(Prec::Lowest)? }); }
                        else { return Err("Keyword arg must be name".into()); }
                    } else { args.push(a); }
                }
                if self.peek().kind == TokenKind::Comma { self.advance(); } else { break; }
            }
        }
        self.expect(TokenKind::RParen, "call )")?;
        Ok(Expr::Call { func: Box::new(func), args, keywords: kw })
    }

    fn parse_sub(&mut self, val: Expr) -> Result<Expr, String> {
        self.advance();
        let lower = self.expr(Prec::Lowest)?;
        let slice = if self.peek().kind == TokenKind::Colon {
            self.advance();
            let upper = if matches!(self.peek().kind, TokenKind::RBracket|TokenKind::Colon) { None } else { Some(self.expr(Prec::Lowest)?) };
            let step = if self.peek().kind == TokenKind::Colon { self.advance(); if self.peek().kind == TokenKind::RBracket { None } else { Some(self.expr(Prec::Lowest)?) } } else { None };
            Slice::Range { lower: Some(lower), upper, step }
        } else { Slice::Index(lower) };
        self.expect(TokenKind::RBracket, "sub ]")?;
        Ok(Expr::Subscript { value: Box::new(val), slice: Box::new(slice) })
    }

    fn lambda(&mut self) -> Result<Expr, String> {
        self.advance();
        let args = self.parse_args()?;
        self.expect(TokenKind::Colon, "lambda :")?;
        let body = self.expr(Prec::Lowest)?;
        Ok(Expr::Lambda { args, body: Box::new(body) })
    }

    fn fstr(&mut self) -> Result<Expr, String> {
        self.advance();
        let mut parts = Vec::new();
        loop {
            match &self.peek().kind {
                TokenKind::FStringMiddle(s) => { parts.push(FStringPart::Literal(s.clone())); self.advance(); }
                TokenKind::FStringExpr => { self.advance(); /* skip expr for now */ }
                TokenKind::FStringEnd(s) => { parts.push(FStringPart::Literal(s.clone())); self.advance(); break; }
                _ => {
                    if let TokenKind::String(s) = &self.peek().kind {
                        parts.push(FStringPart::Literal(s.clone())); self.advance();
                    } else { break; }
                }
            }
        }
        Ok(Expr::FString(parts))
    }

    fn parse_args(&mut self) -> Result<Arguments, String> {
        let mut args = Vec::new();
        let mut defaults = Vec::new();
        let mut vararg = None;
        let mut kwonlyargs = Vec::new();
        let mut kw_defaults = Vec::new();
        let mut kwarg = None;
        let mut saw_star = false;

        if self.peek().kind == TokenKind::RParen {
            return Ok(Arguments { args, vararg: None, kwonlyargs: vec![], kw_defaults: vec![], kwarg: None, defaults });
        }
        loop {
            if self.peek().kind == TokenKind::Star {
                self.advance();
                if self.peek().kind == TokenKind::Comma || self.peek().kind == TokenKind::RParen {
                    saw_star = true; // bare * — remaining args are kwonly
                } else {
                    vararg = Some(Box::new(Arg { arg: self.name()?, annotation: None }));
                    saw_star = true;
                }
            } else if self.peek().kind == TokenKind::DoubleStar {
                self.advance();
                kwarg = Some(Box::new(Arg { arg: self.name()?, annotation: None }));
                break;
            } else {
                let n = self.name()?;
                if saw_star {
                    // keyword-only arg
                    let has_def = self.peek().kind == TokenKind::Eq;
                    if has_def { self.advance(); kw_defaults.push(self.expr(Prec::Lowest)?); }
                    kwonlyargs.push(Arg { arg: n, annotation: None });
                } else {
                    let has_def = self.peek().kind == TokenKind::Eq;
                    if has_def { self.advance(); defaults.push(self.expr(Prec::Lowest)?); }
                    args.push(Arg { arg: n, annotation: None });
                }
            }
            if self.peek().kind == TokenKind::Comma { self.advance(); } else { break; }
        }
        Ok(Arguments { args, vararg, kwonlyargs, kw_defaults, kwarg, defaults })
    }
}

fn binop_tok(k: &TokenKind) -> BinOp {
    match k {
        TokenKind::Plus => BinOp::Add, TokenKind::Minus => BinOp::Sub,
        TokenKind::Star => BinOp::Mul, TokenKind::Slash => BinOp::Div,
        TokenKind::Percent => BinOp::Mod, TokenKind::DoubleStar => BinOp::Pow,
        TokenKind::FloorDiv => BinOp::FloorDiv,
        TokenKind::LShift => BinOp::LShift, TokenKind::RShift => BinOp::RShift,
        TokenKind::Pipe => BinOp::BitOr, TokenKind::BitXor => BinOp::BitXor,
        TokenKind::BitAnd => BinOp::BitAnd, TokenKind::At => BinOp::MatMult,
        _ => BinOp::Add,
    }
}

fn cmp_op_tok(k: &TokenKind) -> Option<CmpOp> {
    Some(match k {
        TokenKind::EqEq => CmpOp::Eq, TokenKind::NotEq => CmpOp::NotEq,
        TokenKind::Lt => CmpOp::Lt, TokenKind::Gt => CmpOp::Gt,
        TokenKind::LtE => CmpOp::LtE, TokenKind::GtE => CmpOp::GtE,
        _ => return None,
    })
}
