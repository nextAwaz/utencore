//! Python 3 AST types.
//!
//! Full AST for statements, expressions, and patterns.

use std::fmt;

// ── Top level ──

#[derive(Debug, Clone)]
pub struct Program {
    pub stmts: Vec<Stmt>,
}

// ── Statements ──

#[derive(Debug, Clone)]
pub enum Stmt {
    // docstring / expression
    Expr(Expr),
    // assignments
    Assign { targets: Vec<Expr>, value: Expr, aug: Option<BinOp> },
    AnnAssign { target: Expr, annotation: Expr, value: Option<Expr> },
    // control flow
    If { test: Expr, body: Vec<Stmt>, orelse: Vec<Stmt> },
    While { test: Expr, body: Vec<Stmt>, orelse: Vec<Stmt> },
    For { target: Expr, iter: Expr, body: Vec<Stmt>, orelse: Vec<Stmt> },
    // function / class
    FuncDef { name: String, args: Arguments, body: Vec<Stmt>, decorators: Vec<Expr>, returns: Option<Expr> },
    ClassDef { name: String, bases: Vec<Expr>, body: Vec<Stmt>, decorators: Vec<Expr> },
    // return / yield
    Return(Option<Expr>),
    Yield(Option<Box<Expr>>),
    YieldFrom(Expr),
    // import
    Import { names: Vec<Alias> },
    ImportFrom { module: Option<String>, names: Vec<Alias>, level: usize },
    // exception handling
    Try { body: Vec<Stmt>, handlers: Vec<ExceptHandler>, orelse: Vec<Stmt>, finalbody: Vec<Stmt> },
    Raise { exc: Option<Expr>, cause: Option<Expr> },
    // with
    With { items: Vec<WithItem>, body: Vec<Stmt> },
    // simple
    Pass, Break, Continue,
    // global / nonlocal / del
    Global(Vec<String>),
    Nonlocal(Vec<String>),
    Del(Expr),
    // assert
    Assert { test: Expr, msg: Option<Expr> },
}

#[derive(Debug, Clone)]
pub struct WithItem {
    pub context_expr: Expr,
    pub optional_vars: Option<Expr>,
}

#[derive(Debug, Clone)]
pub struct ExceptHandler {
    pub typ: Option<Expr>,
    pub name: Option<String>,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone)]
pub struct Alias {
    pub name: String,
    pub asname: Option<String>,
}

// ── Function arguments ──

#[derive(Debug, Clone)]
pub struct Arg {
    pub arg: String,
    pub annotation: Option<Box<Expr>>,
}

#[derive(Debug, Clone)]
pub struct Arguments {
    pub args: Vec<Arg>,
    pub vararg: Option<Box<Arg>>,
    pub kwonlyargs: Vec<Arg>,
    pub kw_defaults: Vec<Expr>,
    pub kwarg: Option<Box<Arg>>,
    pub defaults: Vec<Expr>,
}

// ── Expressions ──

#[derive(Debug, Clone)]
pub enum Expr {
    // literals
    Name(String),
    Int(i64),
    Float(f64),
    Complex { real: f64, imag: f64 },
    Str(String),
    Bytes(Vec<u8>),
    Bool(bool),
    None_,
    Ellipsis,
    FString(Vec<FStringPart>),
    // containers
    List(Vec<Expr>),
    Tuple(Vec<Expr>),
    Set(Vec<Expr>),
    Dict(Vec<(Box<Expr>, Box<Expr>)>),
    // comprehensions
    ListComp { elt: Box<Expr>, generators: Vec<Comprehension> },
    SetComp { elt: Box<Expr>, generators: Vec<Comprehension> },
    DictComp { key: Box<Expr>, value: Box<Expr>, generators: Vec<Comprehension> },
    Generator(Box<Expr>, Vec<Comprehension>),
    // unary
    UnaryOp { op: UnaryOp, operand: Box<Expr> },
    // binary
    BinOp { left: Box<Expr>, op: BinOp, right: Box<Expr> },
    BoolOp { op: BoolOp, values: Vec<Expr> },
    // comparison (chain: a < b < c)
    Compare { left: Box<Expr>, ops: Vec<CmpOp>, comparators: Vec<Expr> },
    // ternary
    IfExpr { test: Box<Expr>, body: Box<Expr>, orelse: Box<Expr> },
    // call / attribute / subscript
    Call { func: Box<Expr>, args: Vec<Expr>, keywords: Vec<Keyword> },
    Attribute { value: Box<Expr>, attr: String },
    Subscript { value: Box<Expr>, slice: Box<Slice> },
    // lambda
    Lambda { args: Arguments, body: Box<Expr> },
    // yield / await
    Yield(Option<Box<Expr>>),
    YieldFrom(Box<Expr>),
    Await(Box<Expr>),
    // starred
    Starred(Box<Expr>),
    // walrus
    NamedExpr { target: Box<Expr>, value: Box<Expr> },
}

#[derive(Debug, Clone)]
pub enum FStringPart {
    Literal(String),
    Expr(Expr),
}

#[derive(Debug, Clone)]
pub struct Comprehension {
    pub target: Box<Expr>,
    pub iter: Box<Expr>,
    pub ifs: Vec<Expr>,
}

#[derive(Debug, Clone)]
pub enum Slice {
    Index(Expr),
    Range { lower: Option<Expr>, upper: Option<Expr>, step: Option<Expr> },
}

#[derive(Debug, Clone)]
pub struct Keyword {
    pub name: Option<String>,
    pub value: Expr,
}

// ── Operators ──

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnaryOp { Not, Invert, UAdd, USub }

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinOp {
    Add, Sub, Mul, Div, Mod, Pow, FloorDiv,
    LShift, RShift, BitOr, BitXor, BitAnd,
    MatMult,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BoolOp { And, Or }

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CmpOp {
    Eq, NotEq, Lt, LtE, Gt, GtE, Is, IsNot, In, NotIn,
}
