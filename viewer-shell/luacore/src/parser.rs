//! A Lua 5.1 (LuaJIT) parser, with Teal's types read and set aside (was
//! `wraithguard/lua/parser.py`, which stays as the fallback and the reference this is
//! held to - node for node, positions and errors included). The node kinds and their
//! children are the Python module's; its docstring lists them.

use super::lexer::{LexError, Token, py_repr, tokenize};

/// One node of the syntax tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    pub kind: &'static str,
    pub line: usize,
    pub col: usize,
    pub value: Option<String>,
    pub children: Vec<Node>,
}

impl Node {
    fn new(kind: &'static str, line: usize, col: usize, value: Option<String>, children: Vec<Node>) -> Self {
        Node { kind, line, col, value, children }
    }
}

type PResult<T> = Result<T, LexError>;

/// (left, right) binding power, from the Lua 5.1 sources (lparser.c `priority[]`).
fn binary(op: &str) -> Option<(u8, u8)> {
    Some(match op {
        "or" => (1, 1),
        "and" => (2, 2),
        "<" | ">" | "<=" | ">=" | "~=" | "==" => (3, 3),
        ".." => (5, 4),
        "+" | "-" => (6, 6),
        "*" | "/" | "%" => (7, 7),
        "^" => (10, 9),
        _ => return None,
    })
}

/// Teal's Lua 5.3 operators on the same scale.
fn binary_teal(op: &str) -> Option<(u8, u8)> {
    Some(match op {
        "|" | "~" | "&" | "<<" | ">>" => (4, 4),
        "//" => (7, 7),
        _ => return None,
    })
}

const UNARY_PRIORITY: u8 = 8;
const NOT_51: [&str; 5] = ["//", "&", "|", "<<", ">>"];
const TYPE_DECLS: [&str; 4] = ["record", "enum", "interface", "type"];
const BLOCK_END: [&str; 4] = ["end", "else", "elseif", "until"];

struct Parser {
    toks: Vec<Token>,
    i: usize,
    teal: bool,
}

fn or_eof(text: &str) -> &str {
    if text.is_empty() { "<eof>" } else { text }
}

impl Parser {
    fn tok(&self) -> &Token {
        &self.toks[self.i]
    }

    fn peek(&self, offset: usize) -> &Token {
        &self.toks[(self.i + offset).min(self.toks.len() - 1)]
    }

    fn check(&self, text: &str) -> bool {
        let t = self.tok();
        (t.0 == "keyword" || t.0 == "op") && t.1 == text
    }

    fn accept(&mut self, text: &str) -> bool {
        if self.check(text) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn error(&self, message: String) -> LexError {
        (message, self.tok().3, self.tok().4)
    }

    fn expect(&mut self, text: &str, opener: Option<&Token>) -> PResult<Token> {
        let t = self.tok().clone();
        if !self.check(text) {
            let wh = opener.map(|o| format!(" (to close {} at line {})", py_repr(&o.1), o.3)).unwrap_or_default();
            return Err(self.error(format!("{} expected{wh} near {}", py_repr(text), py_repr(or_eof(&t.1)))));
        }
        self.i += 1;
        Ok(t)
    }

    fn name(&mut self) -> PResult<Token> {
        let t = self.tok().clone();
        if t.0 != "name" {
            return Err(self.error(format!("name expected near {}", py_repr(or_eof(&t.1)))));
        }
        self.i += 1;
        Ok(t)
    }

    fn node(kind: &'static str, at: &Token, value: Option<String>, kids: Vec<Node>) -> Node {
        Node::new(kind, at.3, at.4, value, kids)
    }

    fn is_kw(t: &Token, words: &[&str]) -> bool {
        t.0 == "keyword" && words.contains(&t.1.as_str())
    }

    // -- blocks and statements -------------------------------------------------------

    fn block(&mut self) -> PResult<Node> {
        let start = self.tok().clone();
        let mut out = Self::node("Block", &start, None, vec![]);
        loop {
            let t = self.tok().clone();
            if t.0 == "eof" || Self::is_kw(&t, &BLOCK_END) {
                return Ok(out);
            }
            if Self::is_kw(&t, &["return"]) {
                out.children.push(self.return_stat()?);
                return Ok(out);
            }
            if Self::is_kw(&t, &["break"]) {
                self.i += 1;
                out.children.push(Self::node("Break", &t, None, vec![]));
                self.accept(";");
                continue;
            }
            if let Some(stmt) = self.statement()? {
                out.children.push(stmt);
            }
        }
    }

    fn return_stat(&mut self) -> PResult<Node> {
        let t = self.tok().clone();
        self.i += 1;
        let mut exprs = Self::node("Exprs", &self.tok().clone(), None, vec![]);
        let nxt = self.tok().clone();
        let ends = nxt.0 == "eof" || Self::is_kw(&nxt, &BLOCK_END);
        if !(ends || self.check(";")) {
            exprs.children = self.exprlist()?;
        }
        self.accept(";");
        Ok(Self::node("Return", &t, None, vec![exprs]))
    }

    fn statement(&mut self) -> PResult<Option<Node>> {
        let t = self.tok().clone();
        if self.accept(";") {
            return Ok(None);
        }
        if t.0 == "keyword" {
            let stmt = match t.1.as_str() {
                "if" => Some(self.if_stat()?),
                "while" => Some(self.while_stat()?),
                "do" => Some(self.do_stat()?),
                "for" => Some(self.for_stat()?),
                "repeat" => Some(self.repeat_stat()?),
                "function" => Some(self.function_stat()?),
                "local" => Some(self.local_stat("local")?),
                "goto" => Some(self.goto_stat()?),
                _ => None,
            };
            if stmt.is_some() {
                return Ok(stmt);
            }
        }
        if self.check("::") {
            self.i += 1;
            let name = self.name()?;
            self.expect("::", None)?;
            return Ok(Some(Self::node("Label", &t, Some(name.1), vec![])));
        }
        if self.teal && t.0 == "name" && t.1 == "global" && self.peek(1).0 != "op" {
            return Ok(Some(self.local_stat("global")?));
        }
        Ok(Some(self.expr_stat()?))
    }

    fn if_stat(&mut self) -> PResult<Node> {
        let start = self.tok().clone();
        let mut out = Self::node("If", &start, None, vec![]);
        loop {
            let clause_tok = self.tok().clone();
            self.i += 1;
            let cond = self.expr(0)?;
            self.expect("then", None)?;
            let body = self.block()?;
            out.children.push(Self::node("Clause", &clause_tok, None, vec![cond, body]));
            if !self.check("elseif") {
                break;
            }
        }
        if self.check("else") {
            let else_tok = self.tok().clone();
            self.i += 1;
            let body = self.block()?;
            out.children.push(Self::node("Else", &else_tok, None, vec![body]));
        }
        self.expect("end", Some(&start))?;
        Ok(out)
    }

    fn while_stat(&mut self) -> PResult<Node> {
        let start = self.tok().clone();
        self.i += 1;
        let cond = self.expr(0)?;
        self.expect("do", None)?;
        let body = self.block()?;
        self.expect("end", Some(&start))?;
        Ok(Self::node("While", &start, None, vec![cond, body]))
    }

    fn do_stat(&mut self) -> PResult<Node> {
        let start = self.tok().clone();
        self.i += 1;
        let body = self.block()?;
        self.expect("end", Some(&start))?;
        Ok(Self::node("Do", &start, None, vec![body]))
    }

    fn repeat_stat(&mut self) -> PResult<Node> {
        let start = self.tok().clone();
        self.i += 1;
        let body = self.block()?;
        self.expect("until", Some(&start))?;
        let cond = self.expr(0)?;
        Ok(Self::node("Repeat", &start, None, vec![body, cond]))
    }

    fn for_stat(&mut self) -> PResult<Node> {
        let start = self.tok().clone();
        self.i += 1;
        let first = self.name()?;
        if self.accept("=") {
            let mut parts = vec![Self::node("Name", &first, Some(first.1.clone()), vec![]), self.expr(0)?];
            self.expect(",", None)?;
            parts.push(self.expr(0)?);
            if self.accept(",") {
                parts.push(self.expr(0)?);
            }
            self.expect("do", None)?;
            parts.push(self.block()?);
            self.expect("end", Some(&start))?;
            return Ok(Self::node("ForNum", &start, None, parts));
        }
        let mut names = Self::node("Names", &first, None, vec![Self::node("Name", &first, Some(first.1.clone()), vec![])]);
        while self.accept(",") {
            let n = self.name()?;
            names.children.push(Self::node("Name", &n, Some(n.1.clone()), vec![]));
        }
        self.expect("in", None)?;
        let at = self.tok().clone();
        let exprs = Self::node("Exprs", &at, None, self.exprlist()?);
        self.expect("do", None)?;
        let body = self.block()?;
        self.expect("end", Some(&start))?;
        Ok(Self::node("ForIn", &start, None, vec![names, exprs, body]))
    }

    fn function_stat(&mut self) -> PResult<Node> {
        let start = self.tok().clone();
        self.i += 1;
        let mut parts = self.name()?.1;
        let mut method = false;
        while self.check(".") || self.check(":") {
            let sep = self.tok().1.clone();
            self.i += 1;
            parts.push_str(&sep);
            parts.push_str(&self.name()?.1);
            if sep == ":" {
                method = true;
                break;
            }
        }
        let f = self.function_body(&start, method)?;
        Ok(Self::node("FunctionStat", &start, Some(parts), vec![f]))
    }

    fn local_stat(&mut self, scope: &str) -> PResult<Node> {
        let start = self.tok().clone();
        self.i += 1;
        if self.accept("function") {
            let name = self.name()?;
            let kind = if scope == "local" { "LocalFunction" } else { "FunctionStat" };
            let f = self.function_body(&start, false)?;
            return Ok(Self::node(kind, &start, Some(name.1), vec![f]));
        }
        let t = self.tok().clone();
        if self.teal && t.0 == "name" && self.peek(1).0 == "name" {
            if TYPE_DECLS.contains(&t.1.as_str()) {
                return self.type_decl();
            }
            if t.1 == "macroexp" {
                self.i += 1;
                let name = self.name()?;
                let f = self.function_body(&start, false)?;
                return Ok(Self::node("LocalFunction", &start, Some(name.1), vec![f]));
            }
        }
        let first = self.name()?;
        let mut names = Self::node("Names", &first, None, vec![Self::node("Name", &first, Some(first.1.clone()), vec![])]);
        self.annotation()?;
        while self.accept(",") {
            if self.teal && self.tok().0 != "name" {
                self.skip_type(true)?;
                continue;
            }
            let n = self.name()?;
            names.children.push(Self::node("Name", &n, Some(n.1.clone()), vec![]));
            self.annotation()?;
        }
        let at = self.tok().clone();
        let mut exprs = Self::node("Exprs", &at, None, vec![]);
        if self.accept("=") {
            exprs.children = self.exprlist()?;
        }
        let value = if scope == "local" { None } else { Some(scope.to_string()) };
        Ok(Self::node("Local", &start, value, vec![names, exprs]))
    }

    // -- Teal's types, read and set aside ----------------------------------------------------

    fn annotation(&mut self) -> PResult<()> {
        if !self.teal {
            return Ok(());
        }
        if self.check("<") {
            self.i += 1;
            self.name()?;
            self.expect(">", None)?;
        }
        if self.accept(":") {
            self.skip_type(true)?;
        }
        Ok(())
    }

    fn is_method_call(&self) -> bool {
        let after = self.peek(3);
        self.peek(2).0 == "name" && (after.1 == "(" || after.1 == "{" || after.0 == "string")
    }

    fn type_decl(&mut self) -> PResult<Node> {
        let kw = self.tok().clone();
        self.i += 1;
        let name = self.name()?;
        if kw.1 == "type" {
            if self.check("<") {
                self.skip_brackets("<", ">")?;
            }
            if self.accept("=") {
                let t = self.tok().clone();
                if t.0 == "name" && t.1 == "require" {
                    self.expr(0)?;
                } else {
                    self.skip_type(true)?;
                }
            }
        } else {
            if self.check("<") {
                self.skip_brackets("<", ">")?;
            }
            self.skip_decl_body(&kw)?;
        }
        Ok(Self::node("TypeDecl", &kw, Some(format!("{} {}", kw.1, name.1)), vec![]))
    }

    fn skip_decl_body(&mut self, opener: &Token) -> PResult<()> {
        let mut depth = 1;
        while depth > 0 {
            let t = self.tok().clone();
            if t.0 == "eof" {
                return Err((format!("{} at line {} is never closed", opener.1, opener.3), t.3, t.4));
            }
            if t.0 == "keyword" && t.1 == "end" {
                depth -= 1;
            } else if t.0 == "name" && matches!(t.1.as_str(), "record" | "enum" | "interface") {
                let nxt = self.peek(1);
                if nxt.0 == "name" || nxt.1 == "<" {
                    depth += 1;
                }
            }
            self.i += 1;
        }
        Ok(())
    }

    fn skip_brackets(&mut self, open: &str, close: &str) -> PResult<()> {
        let start = self.tok().clone();
        let mut depth: i32 = 0;
        loop {
            let t = self.tok().clone();
            if t.0 == "eof" {
                return Err((format!("{} is never closed", py_repr(open)), start.3, start.4));
            }
            if t.0 == "op" {
                if t.1 == open {
                    depth += 1;
                } else if t.1 == close {
                    depth -= 1;
                } else if open == "<" && t.1 == ">>" {
                    depth -= 2;
                }
            }
            self.i += 1;
            if depth <= 0 {
                return Ok(());
            }
        }
    }

    fn skip_type(&mut self, multi: bool) -> PResult<()> {
        self.skip_type_atom(multi)?;
        while self.accept("|") {
            self.skip_type_atom(multi)?;
        }
        Ok(())
    }

    fn skip_type_atom(&mut self, multi: bool) -> PResult<()> {
        let t = self.tok().clone();
        if t.0 == "keyword" && t.1 == "function" {
            self.i += 1;
            if self.check("<") {
                self.skip_brackets("<", ">")?;
            }
            if self.check("(") {
                self.skip_brackets("(", ")")?;
            }
            if self.accept(":") {
                if multi {
                    self.skip_returns()?;
                } else {
                    self.skip_type(false)?;
                    self.accept("...");
                }
            }
            return Ok(());
        }
        if t.0 == "string" || t.1 == "nil" || t.1 == "..." {
            self.i += 1;
            return Ok(());
        }
        if self.check("{") {
            return self.skip_brackets("{", "}");
        }
        if self.check("(") {
            return self.skip_brackets("(", ")");
        }
        if t.0 == "name" && matches!(t.1.as_str(), "record" | "enum" | "interface") {
            self.i += 1;
            return self.skip_decl_body(&t);
        }
        self.name()?;
        while self.accept(".") {
            self.name()?;
        }
        if self.check("<") {
            self.skip_brackets("<", ">")?;
        }
        Ok(())
    }

    fn skip_returns(&mut self) -> PResult<()> {
        self.skip_type(false)?;
        if self.accept("...") {
            return Ok(());
        }
        while self.accept(",") {
            self.skip_type(false)?;
        }
        self.accept("...");
        Ok(())
    }

    fn goto_stat(&mut self) -> PResult<Node> {
        let start = self.tok().clone();
        if self.peek(1).0 != "name" {
            return self.expr_stat();
        }
        self.i += 1;
        let name = self.name()?;
        Ok(Self::node("Goto", &start, Some(name.1), vec![]))
    }

    fn expr_stat(&mut self) -> PResult<Node> {
        let start = self.tok().clone();
        let target = self.suffixed_expr()?;
        if self.check("=") || self.check(",") {
            let mut targets = Self::node("Targets", &start, None, vec![target]);
            while self.accept(",") {
                targets.children.push(self.suffixed_expr()?);
            }
            for t in &targets.children {
                if t.kind != "Name" && t.kind != "Index" {
                    return Err(("cannot assign to this".into(), t.line, t.col));
                }
            }
            self.expect("=", None)?;
            let at = self.tok().clone();
            let exprs = Self::node("Exprs", &at, None, self.exprlist()?);
            return Ok(Self::node("Assign", &start, None, vec![targets, exprs]));
        }
        if target.kind != "Call" && target.kind != "Method" {
            return Err(("a statement cannot start here".into(), start.3, start.4));
        }
        Ok(Self::node("CallStat", &start, None, vec![target]))
    }

    // -- expressions ---------------------------------------------------------------------------

    fn exprlist(&mut self) -> PResult<Vec<Node>> {
        let mut out = vec![self.expr(0)?];
        while self.accept(",") {
            out.push(self.expr(0)?);
        }
        Ok(out)
    }

    fn expr(&mut self, limit: u8) -> PResult<Node> {
        let t = self.tok().clone();
        let unary = (t.0 == "keyword" && t.1 == "not") || (t.0 == "op" && matches!(t.1.as_str(), "-" | "#" | "~"));
        let mut left = if unary {
            if t.1 == "~" && !self.teal {
                return Err(self.error("'~' (bitwise not) is not Lua 5.1 / LuaJIT".into()));
            }
            self.i += 1;
            let operand = self.expr(UNARY_PRIORITY)?;
            Self::node("Unop", &t, Some(t.1.clone()), vec![operand])
        } else {
            self.simple_expr()?
        };
        loop {
            let op = self.tok().clone();
            if self.teal && op.0 == "name" && (op.1 == "as" || op.1 == "is") {
                self.i += 1;
                self.skip_type(false)?;
                left = Node::new("Cast", left.line, left.col, Some(op.1.clone()), vec![left]);
                continue;
            }
            if op.0 != "op" && op.0 != "keyword" {
                return Ok(left);
            }
            let teal_op = if self.teal && op.0 == "op" { binary_teal(&op.1) } else { None };
            if teal_op.is_none() && (NOT_51.contains(&op.1.as_str()) || (op.0 == "op" && op.1 == "~")) {
                return Err(self.error(format!("{} is not Lua 5.1 / LuaJIT (it is Lua 5.3)", py_repr(&op.1))));
            }
            let Some(prio) = teal_op.or_else(|| binary(&op.1)) else { return Ok(left) };
            if prio.0 <= limit {
                return Ok(left);
            }
            self.i += 1;
            let right = self.expr(prio.1)?;
            left = Node::new("Binop", left.line, left.col, Some(op.1.clone()), vec![left, right]);
        }
    }

    fn simple_expr(&mut self) -> PResult<Node> {
        let t = self.tok().clone();
        if t.0 == "keyword" || t.0 == "op" {
            let kind = match t.1.as_str() {
                "nil" => Some("Nil"),
                "true" => Some("True"),
                "false" => Some("False"),
                "..." => Some("Vararg"),
                _ => None,
            };
            if let Some(kind) = kind {
                self.i += 1;
                return Ok(Self::node(kind, &t, None, vec![]));
            }
        }
        if t.0 == "number" {
            self.i += 1;
            return Ok(Self::node("Number", &t, Some(t.5.clone()), vec![]));
        }
        if t.0 == "string" {
            self.i += 1;
            return Ok(Self::node("String", &t, Some(t.5.clone()), vec![]));
        }
        if self.check("{") {
            return self.table();
        }
        if self.check("function") {
            self.i += 1;
            return self.function_body(&t, false);
        }
        self.suffixed_expr()
    }

    fn primary_expr(&mut self) -> PResult<Node> {
        let t = self.tok().clone();
        if t.0 == "name" {
            self.i += 1;
            return Ok(Self::node("Name", &t, Some(t.1.clone()), vec![]));
        }
        if self.accept("(") {
            let inner = self.expr(0)?;
            self.expect(")", Some(&t))?;
            return Ok(Self::node("Paren", &t, None, vec![inner]));
        }
        Err(self.error(format!("unexpected symbol near {}", py_repr(or_eof(&t.1)))))
    }

    fn suffixed_expr(&mut self) -> PResult<Node> {
        let mut e = self.primary_expr()?;
        loop {
            let t = self.tok().clone();
            if self.accept(".") {
                let key = self.name()?;
                let k = Self::node("String", &key, Some(key.1.clone()), vec![]);
                e = Node::new("Index", e.line, e.col, Some(".".into()), vec![e, k]);
            } else if self.accept("[") {
                let key = self.expr(0)?;
                self.expect("]", Some(&t))?;
                e = Node::new("Index", e.line, e.col, Some("[]".into()), vec![e, key]);
            } else if self.accept(":") {
                let m = self.name()?;
                let mut kids = vec![e];
                let (line, col) = (kids[0].line, kids[0].col);
                kids.extend(self.call_args()?);
                e = Node::new("Method", line, col, Some(m.1), kids);
            } else if self.check("(") || self.check("{") || t.0 == "string" {
                if self.check("(") && t.3 != self.toks[self.i - 1].3 {
                    if self.teal {
                        return Ok(e);
                    }
                    return Err(self.error("ambiguous syntax: '(' on a new line after an expression".into()));
                }
                let (line, col) = (e.line, e.col);
                let mut kids = vec![e];
                kids.extend(self.call_args()?);
                e = Node::new("Call", line, col, None, kids);
            } else {
                return Ok(e);
            }
        }
    }

    fn call_args(&mut self) -> PResult<Vec<Node>> {
        let t = self.tok().clone();
        if t.0 == "string" {
            self.i += 1;
            return Ok(vec![Self::node("String", &t, Some(t.5.clone()), vec![])]);
        }
        if self.check("{") {
            return Ok(vec![self.table()?]);
        }
        self.expect("(", None)?;
        if self.accept(")") {
            return Ok(vec![]);
        }
        let args = self.exprlist()?;
        self.expect(")", Some(&t))?;
        Ok(args)
    }

    fn table(&mut self) -> PResult<Node> {
        let start = self.expect("{", None)?;
        let mut out = Self::node("Table", &start, None, vec![]);
        while !self.check("}") {
            let t = self.tok().clone();
            if self.accept("[") {
                let key = self.expr(0)?;
                self.expect("]", Some(&t))?;
                self.expect("=", None)?;
                let value = self.expr(0)?;
                out.children.push(Self::node("Field", &t, None, vec![key, value]));
            } else if self.teal && t.0 == "name" && self.peek(1).1 == ":" && !self.is_method_call() {
                self.i += 2;
                self.skip_type(false)?;
                self.expect("=", None)?;
                let key = Self::node("String", &t, Some(t.1.clone()), vec![]);
                let value = self.expr(0)?;
                out.children.push(Self::node("Field", &t, Some(t.1.clone()), vec![key, value]));
            } else if t.0 == "name" && self.peek(1).0 == "op" && self.peek(1).1 == "=" {
                self.i += 2;
                let key = Self::node("String", &t, Some(t.1.clone()), vec![]);
                let value = self.expr(0)?;
                out.children.push(Self::node("Field", &t, Some(t.1.clone()), vec![key, value]));
            } else {
                let value = self.expr(0)?;
                out.children.push(Self::node("Item", &t, None, vec![value]));
            }
            if !(self.accept(",") || self.accept(";")) {
                break;
            }
        }
        self.expect("}", Some(&start))?;
        Ok(out)
    }

    fn function_body(&mut self, start: &Token, method: bool) -> PResult<Node> {
        if self.teal && self.check("<") {
            self.skip_brackets("<", ">")?;
        }
        let open = self.expect("(", None)?;
        let mut params = Self::node("Params", &open, None, vec![]);
        if method {
            params.children.push(Self::node("Name", &open, Some("self".into()), vec![]));
        }
        if !self.check(")") {
            loop {
                if self.check("...") {
                    let v = self.tok().clone();
                    params.children.push(Self::node("Vararg", &v, None, vec![]));
                    self.i += 1;
                    if self.teal && self.accept(":") {
                        self.skip_type(false)?;
                    }
                    break;
                }
                let n = self.name()?;
                params.children.push(Self::node("Name", &n, Some(n.1.clone()), vec![]));
                if self.teal {
                    self.accept("?");
                    if self.accept(":") {
                        self.skip_type(false)?;
                    }
                }
                if !self.accept(",") {
                    break;
                }
            }
        }
        self.expect(")", Some(&open))?;
        if self.teal && self.accept(":") {
            self.skip_returns()?;
        }
        let body = self.block()?;
        self.expect("end", Some(start))?;
        Ok(Self::node("Function", start, None, vec![params, body]))
    }
}

/// Parses a Lua chunk (or, with `teal`, a Teal one) to its `Block`.
pub fn parse(src: &str, teal: bool) -> Result<Node, LexError> {
    let toks = tokenize(src, false, teal)?;
    let mut p = Parser { toks, i: 0, teal };
    let chunk = p.block()?;
    if p.tok().0 != "eof" {
        return Err(p.error(format!("{} where the chunk should have ended", py_repr(&p.tok().1))));
    }
    Ok(chunk)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statements_and_expressions() {
        let b = parse("local a, b = 1, 'x'\nfunction t.f:g(x, ...) return x .. 1 + 2 end\nif a then b() elseif c then else end", false).unwrap();
        let kinds: Vec<&str> = b.children.iter().map(|n| n.kind).collect();
        assert_eq!(kinds, ["Local", "FunctionStat", "If"]);
        assert_eq!(b.children[1].value.as_deref(), Some("t.f:g"));
        let ret = &b.children[1].children[0].children[1].children[0];
        assert_eq!(ret.kind, "Return");
        assert_eq!(ret.children[0].children[0].value.as_deref(), Some(".."));
    }

    #[test]
    fn teal_types_are_set_aside_and_errors_say_where() {
        let b = parse("local record P x: number end\nlocal function f(a?: integer): string return a as string end", true).unwrap();
        assert_eq!(b.children[0].kind, "TypeDecl");
        assert_eq!(b.children[0].value.as_deref(), Some("record P"));
        let e = parse("x = = 1", false).unwrap_err();
        assert_eq!((e.1, e.2), (1, 5));
        assert!(parse("a = 1 // 2", false).is_err());
        assert!(parse("a = 1 // 2", true).is_ok());
    }
}
