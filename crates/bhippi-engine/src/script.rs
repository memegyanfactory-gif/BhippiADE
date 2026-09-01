//! Gameplay scripts (ENG-176, **ADR-0030**).
//!
//! A script is a document, so it is parsed, validated and compiled here — in Rust, with
//! spans — and executed by a small stack VM in the webview, beside the world it acts on
//! (ADR-0028). Nothing on the frame path crosses an IPC boundary, and the webview never
//! parses script source (INV-082).
//!
//! The source language is a **documented subset of Rhai**, so the `.rhai` extension the
//! schema has always demanded stays honest. Everything outside the subset is rejected by
//! name, with a line, a column and the thing to write instead — a script that silently does
//! less than it says is the failure this module exists to prevent.

use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::HashMap;
use std::fmt;

/// Instructions one hook may execute before the VM calls it a runaway. A budget makes
/// `while true {}` in an AI-written script a red line in the Output Log rather than a
/// frozen editor.
pub const SCRIPT_STEP_BUDGET: u32 = 200_000;

/// Maximum user-function call depth. Recursion past it is a located fault.
pub const SCRIPT_CALL_DEPTH: u32 = 32;

/// The four lifecycle hooks the runtime calls. A file may define any subset.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ScriptHook {
    OnStart,
    OnUpdate,
    OnCollision,
    OnTrigger,
}

impl ScriptHook {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OnStart => "on_start",
            Self::OnUpdate => "on_update",
            Self::OnCollision => "on_collision",
            Self::OnTrigger => "on_trigger",
        }
    }

    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "on_start" => Self::OnStart,
            "on_update" => Self::OnUpdate,
            "on_collision" => Self::OnCollision,
            "on_trigger" => Self::OnTrigger,
            _ => return None,
        })
    }

    /// The arity the runtime calls this hook with. A mismatch is a compile error, because
    /// discovering it at frame 1 costs the user a play session.
    #[must_use]
    pub fn arity(self) -> usize {
        match self {
            Self::OnStart => 0,
            Self::OnUpdate | Self::OnCollision | Self::OnTrigger => 1,
        }
    }
}

impl fmt::Display for ScriptHook {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A located script failure — compile or runtime. Both carry a file and a line, because
/// "your script is wrong" without a line is not a diagnostic.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct ScriptFault {
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub message: String,
    pub hint: Option<String>,
}

impl ScriptFault {
    fn new(file: &str, line: u32, column: u32, message: impl Into<String>, hint: &str) -> Self {
        Self {
            file: file.to_owned(),
            line,
            column,
            message: message.into(),
            hint: Some(hint.to_owned()),
        }
    }
}

impl fmt::Display for ScriptFault {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}:{}:{}: {}",
            self.file, self.line, self.column, self.message
        )
    }
}

// ---------------------------------------------------------------------------------------
// Host surface
// ---------------------------------------------------------------------------------------

/// One host function: the name a script writes, and how many arguments it takes.
///
/// The list is fixed here, in Rust, and the compiler resolves every call against it. The VM
/// can therefore assume a `CallHost` index is valid and an argument count is right, which is
/// most of why the VM stays small enough to trust.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HostFn {
    pub name: &'static str,
    pub arity: usize,
    pub doc: &'static str,
}

macro_rules! host {
    ($name:literal, $arity:literal, $doc:literal) => {
        HostFn {
            name: $name,
            arity: $arity,
            doc: $doc,
        }
    };
}

/// Everything a script may touch. Order here is *not* an ABI: a compiled program carries
/// the names it actually calls in `hosts`, and `scriptVm.ts` binds by name. Adding or
/// reordering an entry therefore cannot silently repoint a call.
pub const HOST_FNS: &[HostFn] = &[
    // -- identity and logging -----------------------------------------------------------
    host!(
        "self_id",
        0,
        "The id of the entity this script is attached to."
    ),
    host!("log", 1, "Write a line to the Output Log's script channel."),
    host!("time", 0, "Seconds since play started."),
    // -- runtime variables --------------------------------------------------------------
    host!(
        "get_var",
        1,
        "Read a runtime variable, e.g. `player.health`."
    ),
    host!(
        "set_var",
        2,
        "Write a runtime variable; HUD bindings see it next frame."
    ),
    // -- transforms ---------------------------------------------------------------------
    host!("pos_x", 1, "World X of an entity (\"\" means self)."),
    host!("pos_y", 1, "World Y of an entity (\"\" means self)."),
    host!("pos_z", 1, "World Z of an entity (\"\" means self)."),
    host!("set_pos", 4, "Place an entity at (x, y, z)."),
    host!("translate", 4, "Offset an entity by (x, y, z)."),
    host!("rot_y", 1, "Yaw of an entity, in radians."),
    host!("set_rot", 4, "Set an entity's euler rotation, in radians."),
    host!("vel_x", 1, "Linear X velocity."),
    host!("vel_y", 1, "Linear Y velocity."),
    host!("vel_z", 1, "Linear Z velocity."),
    host!("set_vel", 4, "Set an entity's linear velocity."),
    host!(
        "grounded",
        1,
        "Whether a character controller is standing on something."
    ),
    // -- world queries ------------------------------------------------------------------
    host!(
        "find",
        1,
        "Find an entity by name; \"\" when there is none."
    ),
    host!(
        "find_tag",
        1,
        "Find the first entity carrying a tag; \"\" when there is none."
    ),
    host!("name_of", 1, "An entity's name."),
    host!("has_tag", 2, "Whether an entity carries a tag."),
    host!("distance", 2, "Distance between two entities."),
    host!(
        "exists",
        1,
        "Whether an entity is still in the runtime world."
    ),
    // -- world mutation -----------------------------------------------------------------
    host!(
        "spawn",
        4,
        "Spawn a runtime entity from a prefab or `builtin:` mesh at (x, y, z)."
    ),
    host!("destroy", 1, "Remove a runtime entity."),
    host!("play_sound", 1, "Play an audio asset once."),
    host!(
        "load_level",
        1,
        "Travel to a level; Main and the HUD stay persistent."
    ),
    // -- HUD ----------------------------------------------------------------------------
    host!("hud_set", 2, "Set a HUD widget's text or value."),
    host!("hud_show", 2, "Show or hide a HUD widget."),
    // -- input --------------------------------------------------------------------------
    host!("is_action", 1, "Whether a named input action is held."),
    host!(
        "action_pressed",
        1,
        "Whether a named action went down this frame."
    ),
    host!("axis", 1, "A named input axis in [-1, 1]."),
    // -- maths --------------------------------------------------------------------------
    host!("abs", 1, "Absolute value."),
    host!("min", 2, "Smaller of two numbers."),
    host!("max", 2, "Larger of two numbers."),
    host!("clamp", 3, "Clamp a value between a low and a high bound."),
    host!("floor", 1, "Round down."),
    host!("ceil", 1, "Round up."),
    host!("round", 1, "Round to nearest."),
    host!("sqrt", 1, "Square root."),
    host!("sin", 1, "Sine of radians."),
    host!("cos", 1, "Cosine of radians."),
    host!("random", 0, "Deterministic pseudo-random number in [0, 1)."),
    host!("to_string", 1, "Render a value as text."),
];

/// Look a host function up by name.
#[must_use]
pub fn host_fn(name: &str) -> Option<(usize, &'static HostFn)> {
    HOST_FNS
        .iter()
        .enumerate()
        .find(|(_, entry)| entry.name == name)
}

// ---------------------------------------------------------------------------------------
// Bytecode
// ---------------------------------------------------------------------------------------

/// The VM's instruction set. Serialised `snake_case`, matched one-for-one in `scriptVm.ts`.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OpCode {
    /// `a` indexes `numbers`.
    PushNum,
    /// `a` indexes `strings`.
    PushStr,
    /// `a` is 0 or 1.
    PushBool,
    PushUnit,
    /// `a` is a local slot.
    Load,
    /// `a` is a local slot; the value is popped.
    Store,
    Pop,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Neg,
    Not,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    /// `a` is a code index.
    Jump,
    /// Pops; `a` is a code index.
    JumpIfFalse,
    /// Peeks (leaves the value) and jumps when falsy — `&&` short-circuit.
    JumpIfFalsePeek,
    /// Peeks and jumps when truthy — `||` short-circuit.
    JumpIfTruePeek,
    /// `a` indexes the program's own `hosts` list, `b` is the argument count.
    CallHost,
    /// `a` indexes `functions`, `b` is the argument count.
    CallUser,
    Return,
}

/// One instruction. Flat and fixed-width so the wire form is an array of small objects
/// rather than a tagged union the webview would have to narrow.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct Instr {
    pub op: OpCode,
    pub a: i32,
    pub b: i32,
    /// Source line, so a runtime fault is located without a side table.
    pub line: u32,
}

/// A compiled function: where its code starts, how many parameters it takes, and how many
/// local slots its frame needs.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ScriptFunction {
    pub name: String,
    pub entry: u32,
    pub params: u32,
    pub locals: u32,
    pub line: u32,
}

/// One hook's entry point.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ScriptHookEntry {
    pub hook: ScriptHook,
    pub function: u32,
}

/// A compiled script, ready to cross to the webview once, when play starts.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ScriptProgram {
    pub file: String,
    pub code: Vec<Instr>,
    pub numbers: Vec<f64>,
    pub strings: Vec<String>,
    pub functions: Vec<ScriptFunction>,
    /// The host functions this program calls, in `CallHost.a` order. The VM binds these by
    /// name, so the two languages share a vocabulary rather than an index.
    pub hosts: Vec<String>,
    pub hooks: Vec<ScriptHookEntry>,
    pub step_budget: u32,
    pub call_depth: u32,
}

impl ScriptProgram {
    /// Which lifecycle hooks this file actually defines — what the Details panel shows and
    /// what the runtime bothers to call.
    #[must_use]
    pub fn hook_names(&self) -> Vec<&'static str> {
        self.hooks.iter().map(|entry| entry.hook.as_str()).collect()
    }
}

// ---------------------------------------------------------------------------------------
// Lexer
// ---------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Ident(String),
    Number(f64),
    Str(String),
    Fn,
    Let,
    If,
    Else,
    While,
    Return,
    Break,
    Continue,
    True,
    False,
    LParen,
    RParen,
    LBrace,
    RBrace,
    Comma,
    Semi,
    Assign,
    PlusAssign,
    MinusAssign,
    StarAssign,
    SlashAssign,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Bang,
    EqEq,
    BangEq,
    Lt,
    Le,
    Gt,
    Ge,
    AndAnd,
    OrOr,
    Eof,
}

impl Tok {
    fn describe(&self) -> String {
        match self {
            Self::Ident(name) => format!("`{name}`"),
            Self::Number(value) => format!("`{value}`"),
            Self::Str(_) => "a string".to_owned(),
            Self::Eof => "the end of the file".to_owned(),
            other => format!("`{}`", other.symbol()),
        }
    }

    fn symbol(&self) -> &'static str {
        match self {
            Self::Fn => "fn",
            Self::Let => "let",
            Self::If => "if",
            Self::Else => "else",
            Self::While => "while",
            Self::Return => "return",
            Self::Break => "break",
            Self::Continue => "continue",
            Self::True => "true",
            Self::False => "false",
            Self::LParen => "(",
            Self::RParen => ")",
            Self::LBrace => "{",
            Self::RBrace => "}",
            Self::Comma => ",",
            Self::Semi => ";",
            Self::Assign => "=",
            Self::PlusAssign => "+=",
            Self::MinusAssign => "-=",
            Self::StarAssign => "*=",
            Self::SlashAssign => "/=",
            Self::Plus => "+",
            Self::Minus => "-",
            Self::Star => "*",
            Self::Slash => "/",
            Self::Percent => "%",
            Self::Bang => "!",
            Self::EqEq => "==",
            Self::BangEq => "!=",
            Self::Lt => "<",
            Self::Le => "<=",
            Self::Gt => ">",
            Self::Ge => ">=",
            Self::AndAnd => "&&",
            Self::OrOr => "||",
            _ => "",
        }
    }
}

#[derive(Clone, Debug)]
struct Spanned {
    tok: Tok,
    line: u32,
    column: u32,
}

/// Rhai constructs we deliberately do not support. Naming them beats "unexpected token":
/// the author learns the boundary instead of guessing at it.
const UNSUPPORTED: &[(&str, &str)] = &[
    (
        "for",
        "Use `while` with your own counter; `for` needs iterators, which the subset has no values for.",
    ),
    (
        "loop",
        "Use `while true { ... }` — it is bounded by the script step budget either way.",
    ),
    ("switch", "Use `if` / `else if`."),
    (
        "import",
        "Scripts cannot import; put shared helpers in the same file.",
    ),
    (
        "export",
        "Scripts cannot export; the runtime calls the four lifecycle hooks.",
    ),
    (
        "private",
        "Every function in a script file is private to it already.",
    ),
    (
        "throw",
        "Return early instead; faults come from the host, with a line.",
    ),
    ("try", "There is nothing to catch — host calls cannot throw."),
    ("do", "Use `while`."),
    ("in", "Iteration is not in the subset."),
];

fn lex(file: &str, source: &str) -> Result<Vec<Spanned>, ScriptFault> {
    let chars: Vec<char> = source.chars().collect();
    let mut index = 0usize;
    let mut line = 1u32;
    let mut column = 1u32;
    let mut out = Vec::new();

    macro_rules! bump {
        ($count:expr) => {{
            for _ in 0..$count {
                if chars.get(index) == Some(&'\n') {
                    line += 1;
                    column = 1;
                } else {
                    column += 1;
                }
                index += 1;
            }
        }};
    }

    while index < chars.len() {
        let current = chars[index];
        if current.is_whitespace() {
            bump!(1);
            continue;
        }
        if current == '/' && chars.get(index + 1) == Some(&'/') {
            while index < chars.len() && chars[index] != '\n' {
                bump!(1);
            }
            continue;
        }
        if current == '/' && chars.get(index + 1) == Some(&'*') {
            let (start_line, start_column) = (line, column);
            bump!(2);
            loop {
                if index >= chars.len() {
                    return Err(ScriptFault::new(
                        file,
                        start_line,
                        start_column,
                        "This block comment is never closed.",
                        "Add a closing `*/`.",
                    ));
                }
                if chars[index] == '*' && chars.get(index + 1) == Some(&'/') {
                    bump!(2);
                    break;
                }
                bump!(1);
            }
            continue;
        }
        let (start_line, start_column) = (line, column);

        if current.is_ascii_digit()
            || (current == '.' && chars.get(index + 1).is_some_and(char::is_ascii_digit))
        {
            let mut text = String::new();
            while index < chars.len() && (chars[index].is_ascii_digit() || chars[index] == '.') {
                text.push(chars[index]);
                bump!(1);
            }
            let value = text.parse::<f64>().map_err(|_| {
                ScriptFault::new(
                    file,
                    start_line,
                    start_column,
                    format!("`{text}` is not a number."),
                    "Write a decimal literal such as `1`, `2.5` or `0.033`.",
                )
            })?;
            out.push(Spanned {
                tok: Tok::Number(value),
                line: start_line,
                column: start_column,
            });
            continue;
        }

        if current.is_alphabetic() || current == '_' {
            let mut text = String::new();
            while index < chars.len() && (chars[index].is_alphanumeric() || chars[index] == '_') {
                text.push(chars[index]);
                bump!(1);
            }
            if let Some((_, hint)) = UNSUPPORTED.iter().find(|(word, _)| *word == text) {
                return Err(ScriptFault::new(
                    file,
                    start_line,
                    start_column,
                    format!("`{text}` is not in the supported script subset (ADR-0030)."),
                    hint,
                ));
            }
            let tok = match text.as_str() {
                "fn" => Tok::Fn,
                "let" => Tok::Let,
                "if" => Tok::If,
                "else" => Tok::Else,
                "while" => Tok::While,
                "return" => Tok::Return,
                "break" => Tok::Break,
                "continue" => Tok::Continue,
                "true" => Tok::True,
                "false" => Tok::False,
                _ => Tok::Ident(text),
            };
            out.push(Spanned {
                tok,
                line: start_line,
                column: start_column,
            });
            continue;
        }

        if current == '"' {
            bump!(1);
            let mut text = String::new();
            loop {
                if index >= chars.len() || chars[index] == '\n' {
                    return Err(ScriptFault::new(
                        file,
                        start_line,
                        start_column,
                        "This string is never closed.",
                        "Add a closing `\"` on the same line.",
                    ));
                }
                if chars[index] == '\\' {
                    let escaped = chars.get(index + 1).copied().unwrap_or('"');
                    text.push(match escaped {
                        'n' => '\n',
                        't' => '\t',
                        other => other,
                    });
                    bump!(2);
                    continue;
                }
                if chars[index] == '"' {
                    bump!(1);
                    break;
                }
                text.push(chars[index]);
                bump!(1);
            }
            out.push(Spanned {
                tok: Tok::Str(text),
                line: start_line,
                column: start_column,
            });
            continue;
        }

        let two: String = chars[index..(index + 2).min(chars.len())].iter().collect();
        let (tok, width) = match two.as_str() {
            "==" => (Tok::EqEq, 2),
            "!=" => (Tok::BangEq, 2),
            "<=" => (Tok::Le, 2),
            ">=" => (Tok::Ge, 2),
            "&&" => (Tok::AndAnd, 2),
            "||" => (Tok::OrOr, 2),
            "+=" => (Tok::PlusAssign, 2),
            "-=" => (Tok::MinusAssign, 2),
            "*=" => (Tok::StarAssign, 2),
            "/=" => (Tok::SlashAssign, 2),
            _ => match current {
                '(' => (Tok::LParen, 1),
                ')' => (Tok::RParen, 1),
                '{' => (Tok::LBrace, 1),
                '}' => (Tok::RBrace, 1),
                ',' => (Tok::Comma, 1),
                ';' => (Tok::Semi, 1),
                '=' => (Tok::Assign, 1),
                '+' => (Tok::Plus, 1),
                '-' => (Tok::Minus, 1),
                '*' => (Tok::Star, 1),
                '/' => (Tok::Slash, 1),
                '%' => (Tok::Percent, 1),
                '!' => (Tok::Bang, 1),
                '<' => (Tok::Lt, 1),
                '>' => (Tok::Gt, 1),
                other => {
                    return Err(ScriptFault::new(
                        file,
                        start_line,
                        start_column,
                        format!("`{other}` has no meaning in a script."),
                        "The subset is `fn`, `let`, `if`, `while`, `return`, arithmetic and calls (ADR-0030).",
                    ));
                }
            },
        };
        bump!(width);
        out.push(Spanned {
            tok,
            line: start_line,
            column: start_column,
        });
    }

    out.push(Spanned {
        tok: Tok::Eof,
        line,
        column,
    });
    Ok(out)
}

// ---------------------------------------------------------------------------------------
// Compiler
// ---------------------------------------------------------------------------------------

#[derive(Default)]
struct LoopFrame {
    start: u32,
    breaks: Vec<usize>,
    continues: Vec<usize>,
}

struct Compiler<'a> {
    file: &'a str,
    tokens: Vec<Spanned>,
    cursor: usize,
    code: Vec<Instr>,
    numbers: Vec<f64>,
    strings: Vec<String>,
    functions: Vec<ScriptFunction>,
    hosts: Vec<String>,
    /// name -> (function index, parameter count), filled by a pre-pass so functions may
    /// call each other regardless of declaration order.
    signatures: HashMap<String, (u32, usize)>,
    /// Open lexical scopes; each holds the names declared in it, in slot order.
    scopes: Vec<Vec<String>>,
    locals: u32,
    max_locals: u32,
    loops: Vec<LoopFrame>,
}

impl Compiler<'_> {
    fn peek(&self) -> &Tok {
        &self.tokens[self.cursor].tok
    }

    fn at(&self) -> (u32, u32) {
        let span = &self.tokens[self.cursor];
        (span.line, span.column)
    }

    fn line(&self) -> u32 {
        self.tokens[self.cursor].line
    }

    fn advance(&mut self) -> Spanned {
        let span = self.tokens[self.cursor].clone();
        if self.cursor + 1 < self.tokens.len() {
            self.cursor += 1;
        }
        span
    }

    fn eat(&mut self, tok: &Tok) -> bool {
        if self.peek() == tok {
            self.advance();
            true
        } else {
            false
        }
    }

    fn fault(&self, message: impl Into<String>, hint: &str) -> ScriptFault {
        let (line, column) = self.at();
        ScriptFault::new(self.file, line, column, message, hint)
    }

    fn expect(&mut self, tok: &Tok, hint: &str) -> Result<Spanned, ScriptFault> {
        if self.peek() == tok {
            Ok(self.advance())
        } else {
            let found = self.peek().describe();
            Err(self.fault(format!("Expected `{}`, found {found}.", tok.symbol()), hint))
        }
    }

    fn expect_ident(&mut self, hint: &str) -> Result<(String, u32, u32), ScriptFault> {
        let (line, column) = self.at();
        match self.peek().clone() {
            Tok::Ident(name) => {
                self.advance();
                Ok((name, line, column))
            }
            other => Err(self.fault(
                format!("Expected a name, found {}.", other.describe()),
                hint,
            )),
        }
    }

    fn emit(&mut self, op: OpCode, a: i32, b: i32, line: u32) -> usize {
        self.code.push(Instr { op, a, b, line });
        self.code.len() - 1
    }

    fn number(&mut self, value: f64) -> i32 {
        if let Some(index) = self.numbers.iter().position(|entry| *entry == value) {
            return i32::try_from(index).unwrap_or(0);
        }
        self.numbers.push(value);
        i32::try_from(self.numbers.len() - 1).unwrap_or(0)
    }

    fn host_slot(&mut self, name: &str) -> i32 {
        if let Some(index) = self.hosts.iter().position(|entry| entry == name) {
            return i32::try_from(index).unwrap_or(0);
        }
        self.hosts.push(name.to_owned());
        i32::try_from(self.hosts.len() - 1).unwrap_or(0)
    }

    fn string(&mut self, value: &str) -> i32 {
        if let Some(index) = self.strings.iter().position(|entry| entry == value) {
            return i32::try_from(index).unwrap_or(0);
        }
        self.strings.push(value.to_owned());
        i32::try_from(self.strings.len() - 1).unwrap_or(0)
    }

    /// Slots are assigned in declaration order across every open scope, so a name's slot is
    /// the count of everything declared before it.
    fn resolve(&self, name: &str) -> Option<u32> {
        let mut base = 0usize;
        let mut found = None;
        for scope in &self.scopes {
            if let Some(offset) = scope.iter().rposition(|entry| entry == name) {
                found = u32::try_from(base + offset).ok();
            }
            base += scope.len();
        }
        found
    }

    fn declare(&mut self, name: &str) -> u32 {
        let slot = self.locals;
        self.locals += 1;
        self.max_locals = self.max_locals.max(self.locals);
        if let Some(scope) = self.scopes.last_mut() {
            scope.push(name.to_owned());
        }
        slot
    }

    fn push_scope(&mut self) {
        self.scopes.push(Vec::new());
    }

    fn pop_scope(&mut self) {
        if let Some(scope) = self.scopes.pop() {
            self.locals -= u32::try_from(scope.len()).unwrap_or(0);
        }
    }

    fn patch(&mut self, at: usize, target: usize) {
        if let Some(instr) = self.code.get_mut(at) {
            instr.a = i32::try_from(target).unwrap_or(0);
        }
    }

    // -- items --------------------------------------------------------------------------

    fn collect_signatures(&mut self) -> Result<(), ScriptFault> {
        let saved = self.cursor;
        while !matches!(self.peek(), Tok::Eof) {
            if !self.eat(&Tok::Fn) {
                let found = self.peek().describe();
                return Err(self.fault(
                    format!("Expected `fn` at the top level, found {found}."),
                    "A script file contains only `fn` declarations; put statements inside a hook such as `fn on_update(dt) { ... }`.",
                ));
            }
            let (name, line, column) =
                self.expect_ident("Name the function, e.g. `fn on_update(dt)`.")?;
            let params = self.parameter_list()?;
            if let Some(hook) = ScriptHook::from_name(&name) {
                if params.len() != hook.arity() {
                    return Err(ScriptFault::new(
                        self.file,
                        line,
                        column,
                        format!(
                            "`{name}` takes {} argument(s); this one declares {}.",
                            hook.arity(),
                            params.len()
                        ),
                        match hook {
                            ScriptHook::OnStart => "Write `fn on_start()`.",
                            ScriptHook::OnUpdate => {
                                "Write `fn on_update(dt)` — dt is the frame time in seconds."
                            }
                            ScriptHook::OnCollision => {
                                "Write `fn on_collision(other)` — other is the entity id."
                            }
                            ScriptHook::OnTrigger => {
                                "Write `fn on_trigger(other)` — other is the entity id."
                            }
                        },
                    ));
                }
            }
            if self.signatures.contains_key(&name) {
                return Err(ScriptFault::new(
                    self.file,
                    line,
                    column,
                    format!("`{name}` is declared twice."),
                    "Rename one of them; the subset has no overloading.",
                ));
            }
            if host_fn(&name).is_some() {
                return Err(ScriptFault::new(
                    self.file,
                    line,
                    column,
                    format!("`{name}` is a built-in host function and cannot be redefined."),
                    "Pick another name.",
                ));
            }
            let index = u32::try_from(self.functions.len()).unwrap_or(0);
            self.signatures.insert(name.clone(), (index, params.len()));
            self.functions.push(ScriptFunction {
                name,
                entry: 0,
                params: u32::try_from(params.len()).unwrap_or(0),
                locals: 0,
                line,
            });
            self.skip_block()?;
        }
        self.cursor = saved;
        Ok(())
    }

    fn parameter_list(&mut self) -> Result<Vec<String>, ScriptFault> {
        self.expect(&Tok::LParen, "Function parameters go in `( )`.")?;
        let mut params = Vec::new();
        if self.eat(&Tok::RParen) {
            return Ok(params);
        }
        loop {
            let (param, _, _) = self.expect_ident("Parameters are plain names.")?;
            params.push(param);
            if self.eat(&Tok::Comma) {
                continue;
            }
            self.expect(&Tok::RParen, "Close the parameter list with `)`.")?;
            return Ok(params);
        }
    }

    /// Walk past a `{ ... }` without compiling it, for the signature pre-pass.
    fn skip_block(&mut self) -> Result<(), ScriptFault> {
        self.expect(&Tok::LBrace, "A function body is a `{ ... }` block.")?;
        let mut depth = 1usize;
        while depth > 0 {
            match self.peek() {
                Tok::LBrace => depth += 1,
                Tok::RBrace => depth -= 1,
                Tok::Eof => {
                    return Err(self.fault(
                        "This function body is never closed.",
                        "Add the missing `}`.",
                    ));
                }
                _ => {}
            }
            self.advance();
        }
        Ok(())
    }

    fn function(&mut self) -> Result<(), ScriptFault> {
        self.expect(&Tok::Fn, "Only `fn` declarations live at the top level.")?;
        let (name, _, _) = self.expect_ident("Name the function.")?;
        let params = self.parameter_list()?;

        let index = self
            .signatures
            .get(&name)
            .map_or(0usize, |(index, _)| *index as usize);
        let entry = u32::try_from(self.code.len()).unwrap_or(0);
        self.locals = 0;
        self.max_locals = 0;
        self.scopes.clear();
        self.push_scope();
        for param in &params {
            self.declare(param);
        }
        self.block()?;
        // Every path returns: an implicit unit tail keeps the VM's frame contract simple.
        let line = self.line();
        self.emit(OpCode::PushUnit, 0, 0, line);
        self.emit(OpCode::Return, 0, 0, line);
        self.pop_scope();

        if let Some(function) = self.functions.get_mut(index) {
            function.entry = entry;
            function.locals = self.max_locals;
        }
        Ok(())
    }

    // -- statements ---------------------------------------------------------------------

    fn block(&mut self) -> Result<(), ScriptFault> {
        self.expect(&Tok::LBrace, "A block starts with `{`.")?;
        self.push_scope();
        while !matches!(self.peek(), Tok::RBrace | Tok::Eof) {
            self.statement()?;
        }
        self.pop_scope();
        self.expect(&Tok::RBrace, "Close the block with `}`.")?;
        Ok(())
    }

    fn statement(&mut self) -> Result<(), ScriptFault> {
        let line = self.line();
        match self.peek().clone() {
            Tok::Let => self.let_statement(line),
            Tok::Return => {
                self.advance();
                if matches!(self.peek(), Tok::Semi) {
                    self.emit(OpCode::PushUnit, 0, 0, line);
                } else {
                    self.expression()?;
                }
                self.expect(&Tok::Semi, "Statements end with `;`.")?;
                self.emit(OpCode::Return, 0, 0, line);
                Ok(())
            }
            Tok::Break | Tok::Continue => self.loop_jump(line),
            Tok::If => self.if_statement(),
            Tok::While => self.while_statement(line),
            Tok::LBrace => self.block(),
            Tok::Ident(name) => self.ident_statement(&name, line),
            Tok::Semi => {
                self.advance();
                Ok(())
            }
            other => Err(self.fault(
                format!("{} cannot start a statement.", other.describe()),
                "Statements are `let`, an assignment, a call, `if`, `while`, `return`, `break` or `continue`.",
            )),
        }
    }

    fn let_statement(&mut self, line: u32) -> Result<(), ScriptFault> {
        self.advance();
        let (name, decl_line, decl_column) = self.expect_ident("Write `let name = value;`.")?;
        if self
            .scopes
            .last()
            .is_some_and(|scope| scope.contains(&name))
        {
            return Err(ScriptFault::new(
                self.file,
                decl_line,
                decl_column,
                format!("`{name}` is already declared in this block."),
                "Assign to it (`name = ...;`) or pick another name.",
            ));
        }
        self.expect(&Tok::Assign, "A `let` must have a value: `let x = 0;`.")?;
        self.expression()?;
        self.expect(&Tok::Semi, "Statements end with `;`.")?;
        let slot = self.declare(&name);
        self.emit(OpCode::Store, i32::try_from(slot).unwrap_or(0), 0, line);
        Ok(())
    }

    fn loop_jump(&mut self, line: u32) -> Result<(), ScriptFault> {
        let is_break = matches!(self.peek(), Tok::Break);
        self.advance();
        self.expect(&Tok::Semi, "Statements end with `;`.")?;
        if self.loops.is_empty() {
            return Err(ScriptFault::new(
                self.file,
                line,
                1,
                format!(
                    "`{}` is only meaningful inside a `while` loop.",
                    if is_break { "break" } else { "continue" }
                ),
                "Remove it, or wrap the code in `while ... { ... }`.",
            ));
        }
        let at = self.emit(OpCode::Jump, 0, 0, line);
        if let Some(frame) = self.loops.last_mut() {
            if is_break {
                frame.breaks.push(at);
            } else {
                frame.continues.push(at);
            }
        }
        Ok(())
    }

    fn while_statement(&mut self, line: u32) -> Result<(), ScriptFault> {
        self.advance();
        let start = u32::try_from(self.code.len()).unwrap_or(0);
        self.loops.push(LoopFrame {
            start,
            ..LoopFrame::default()
        });
        self.expression()?;
        let exit = self.emit(OpCode::JumpIfFalse, 0, 0, line);
        self.block()?;
        self.emit(OpCode::Jump, i32::try_from(start).unwrap_or(0), 0, line);
        let end = self.code.len();
        self.patch(exit, end);
        let frame = self.loops.pop().unwrap_or_default();
        for at in frame.breaks {
            self.patch(at, end);
        }
        for at in frame.continues {
            self.patch(at, frame.start as usize);
        }
        Ok(())
    }

    fn ident_statement(&mut self, name: &str, line: u32) -> Result<(), ScriptFault> {
        let assigns = matches!(
            self.tokens.get(self.cursor + 1).map(|span| &span.tok),
            Some(
                Tok::Assign
                    | Tok::PlusAssign
                    | Tok::MinusAssign
                    | Tok::StarAssign
                    | Tok::SlashAssign
            )
        );
        if !assigns {
            self.expression()?;
            self.expect(&Tok::Semi, "Statements end with `;`.")?;
            self.emit(OpCode::Pop, 0, 0, line);
            return Ok(());
        }
        let (_, ident_line, ident_column) = self.expect_ident("")?;
        let Some(slot) = self.resolve(name) else {
            return Err(ScriptFault::new(
                self.file,
                ident_line,
                ident_column,
                format!("`{name}` is not declared."),
                "Declare it first with `let`, or use `set_var(\"name\", ...)` for runtime state.",
            ));
        };
        let op = self.advance().tok;
        if !matches!(op, Tok::Assign) {
            self.emit(OpCode::Load, i32::try_from(slot).unwrap_or(0), 0, line);
        }
        self.expression()?;
        match op {
            Tok::PlusAssign => self.emit(OpCode::Add, 0, 0, line),
            Tok::MinusAssign => self.emit(OpCode::Sub, 0, 0, line),
            Tok::StarAssign => self.emit(OpCode::Mul, 0, 0, line),
            Tok::SlashAssign => self.emit(OpCode::Div, 0, 0, line),
            _ => 0,
        };
        self.expect(&Tok::Semi, "Statements end with `;`.")?;
        self.emit(OpCode::Store, i32::try_from(slot).unwrap_or(0), 0, line);
        Ok(())
    }

    fn if_statement(&mut self) -> Result<(), ScriptFault> {
        let line = self.line();
        self.expect(&Tok::If, "")?;
        self.expression()?;
        let to_else = self.emit(OpCode::JumpIfFalse, 0, 0, line);
        self.block()?;
        if self.eat(&Tok::Else) {
            let to_end = self.emit(OpCode::Jump, 0, 0, line);
            let else_at = self.code.len();
            self.patch(to_else, else_at);
            if matches!(self.peek(), Tok::If) {
                self.if_statement()?;
            } else {
                self.block()?;
            }
            let end = self.code.len();
            self.patch(to_end, end);
        } else {
            let end = self.code.len();
            self.patch(to_else, end);
        }
        Ok(())
    }

    // -- expressions --------------------------------------------------------------------

    fn expression(&mut self) -> Result<(), ScriptFault> {
        self.or_expression()
    }

    fn or_expression(&mut self) -> Result<(), ScriptFault> {
        self.and_expression()?;
        while matches!(self.peek(), Tok::OrOr) {
            let line = self.line();
            self.advance();
            let skip = self.emit(OpCode::JumpIfTruePeek, 0, 0, line);
            self.emit(OpCode::Pop, 0, 0, line);
            self.and_expression()?;
            let end = self.code.len();
            self.patch(skip, end);
        }
        Ok(())
    }

    fn and_expression(&mut self) -> Result<(), ScriptFault> {
        self.comparison()?;
        while matches!(self.peek(), Tok::AndAnd) {
            let line = self.line();
            self.advance();
            let skip = self.emit(OpCode::JumpIfFalsePeek, 0, 0, line);
            self.emit(OpCode::Pop, 0, 0, line);
            self.comparison()?;
            let end = self.code.len();
            self.patch(skip, end);
        }
        Ok(())
    }

    fn comparison(&mut self) -> Result<(), ScriptFault> {
        self.additive()?;
        loop {
            let line = self.line();
            let op = match self.peek() {
                Tok::EqEq => OpCode::Eq,
                Tok::BangEq => OpCode::Ne,
                Tok::Lt => OpCode::Lt,
                Tok::Le => OpCode::Le,
                Tok::Gt => OpCode::Gt,
                Tok::Ge => OpCode::Ge,
                _ => return Ok(()),
            };
            self.advance();
            self.additive()?;
            self.emit(op, 0, 0, line);
        }
    }

    fn additive(&mut self) -> Result<(), ScriptFault> {
        self.multiplicative()?;
        loop {
            let line = self.line();
            let op = match self.peek() {
                Tok::Plus => OpCode::Add,
                Tok::Minus => OpCode::Sub,
                _ => return Ok(()),
            };
            self.advance();
            self.multiplicative()?;
            self.emit(op, 0, 0, line);
        }
    }

    fn multiplicative(&mut self) -> Result<(), ScriptFault> {
        self.unary()?;
        loop {
            let line = self.line();
            let op = match self.peek() {
                Tok::Star => OpCode::Mul,
                Tok::Slash => OpCode::Div,
                Tok::Percent => OpCode::Rem,
                _ => return Ok(()),
            };
            self.advance();
            self.unary()?;
            self.emit(op, 0, 0, line);
        }
    }

    fn unary(&mut self) -> Result<(), ScriptFault> {
        let line = self.line();
        match self.peek() {
            Tok::Minus => {
                self.advance();
                self.unary()?;
                self.emit(OpCode::Neg, 0, 0, line);
                Ok(())
            }
            Tok::Bang => {
                self.advance();
                self.unary()?;
                self.emit(OpCode::Not, 0, 0, line);
                Ok(())
            }
            _ => self.primary(),
        }
    }

    fn primary(&mut self) -> Result<(), ScriptFault> {
        let line = self.line();
        match self.peek().clone() {
            Tok::Number(value) => {
                self.advance();
                let index = self.number(value);
                self.emit(OpCode::PushNum, index, 0, line);
                Ok(())
            }
            Tok::Str(value) => {
                self.advance();
                let index = self.string(&value);
                self.emit(OpCode::PushStr, index, 0, line);
                Ok(())
            }
            Tok::True | Tok::False => {
                let truth = i32::from(matches!(self.peek(), Tok::True));
                self.advance();
                self.emit(OpCode::PushBool, truth, 0, line);
                Ok(())
            }
            Tok::LParen => {
                self.advance();
                self.expression()?;
                self.expect(&Tok::RParen, "Close the group with `)`.")?;
                Ok(())
            }
            Tok::Ident(name) => {
                let (_, ident_line, ident_column) = self.expect_ident("")?;
                if self.eat(&Tok::LParen) {
                    let mut argc = 0usize;
                    if !self.eat(&Tok::RParen) {
                        loop {
                            self.expression()?;
                            argc += 1;
                            if self.eat(&Tok::Comma) {
                                continue;
                            }
                            self.expect(&Tok::RParen, "Close the call with `)`.")?;
                            break;
                        }
                    }
                    return self.call(&name, argc, ident_line, ident_column);
                }
                let Some(slot) = self.resolve(&name) else {
                    let nearest = self.nearest_name(&name);
                    return Err(ScriptFault::new(
                        self.file,
                        ident_line,
                        ident_column,
                        format!("`{name}` is not declared."),
                        &nearest,
                    ));
                };
                self.emit(
                    OpCode::Load,
                    i32::try_from(slot).unwrap_or(0),
                    0,
                    ident_line,
                );
                Ok(())
            }
            other => Err(self.fault(
                format!("{} is not a value.", other.describe()),
                "Values are numbers, strings, `true`, `false`, a variable or a call.",
            )),
        }
    }

    fn call(&mut self, name: &str, argc: usize, line: u32, column: u32) -> Result<(), ScriptFault> {
        if let Some((_, host)) = host_fn(name) {
            if argc != host.arity {
                return Err(ScriptFault::new(
                    self.file,
                    line,
                    column,
                    format!("`{name}` takes {} argument(s), not {argc}.", host.arity),
                    host.doc,
                ));
            }
            let slot = self.host_slot(name);
            self.emit(
                OpCode::CallHost,
                slot,
                i32::try_from(argc).unwrap_or(0),
                line,
            );
            return Ok(());
        }
        if let Some((index, params)) = self.signatures.get(name).copied() {
            if argc != params {
                return Err(ScriptFault::new(
                    self.file,
                    line,
                    column,
                    format!("`{name}` takes {params} argument(s), not {argc}."),
                    "Match the declaration's parameter list.",
                ));
            }
            self.emit(
                OpCode::CallUser,
                i32::try_from(index).unwrap_or(0),
                i32::try_from(argc).unwrap_or(0),
                line,
            );
            return Ok(());
        }
        let nearest = self.nearest_name(name);
        Err(ScriptFault::new(
            self.file,
            line,
            column,
            format!("`{name}` is not a host function or a function in this file."),
            &nearest,
        ))
    }

    /// A typo deserves the name it nearly was. Cheap edit distance over the host list and
    /// the file's own functions.
    fn nearest_name(&self, name: &str) -> String {
        let mut best: Option<(usize, &str)> = None;
        let candidates = HOST_FNS
            .iter()
            .map(|entry| entry.name)
            .chain(self.signatures.keys().map(String::as_str));
        for candidate in candidates {
            let distance = edit_distance(name, candidate);
            if distance <= 3 && best.is_none_or(|(score, _)| distance < score) {
                best = Some((distance, candidate));
            }
        }
        best.map_or_else(
            || "See the host function list in `prompts/chat-engine.md` (ADR-0030).".to_owned(),
            |(_, candidate)| format!("Did you mean `{candidate}`?"),
        )
    }
}

fn edit_distance(left: &str, right: &str) -> usize {
    let left: Vec<char> = left.chars().collect();
    let right: Vec<char> = right.chars().collect();
    let mut previous: Vec<usize> = (0..=right.len()).collect();
    let mut current = vec![0usize; right.len() + 1];
    for (i, l) in left.iter().enumerate() {
        current[0] = i + 1;
        for (j, r) in right.iter().enumerate() {
            let cost = usize::from(l != r);
            current[j + 1] = (previous[j + 1] + 1)
                .min(current[j] + 1)
                .min(previous[j] + cost);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[right.len()]
}

/// Compile one script file. `file` is the project-relative path used in every fault.
///
/// # Errors
/// Returns the first `ScriptFault` — with line, column and a hint — that stops the file
/// compiling. Compilation is all-or-nothing per file; a half-loaded script is worse than
/// none, because the half that ran already changed the world.
pub fn compile(file: &str, source: &str) -> Result<ScriptProgram, ScriptFault> {
    let tokens = lex(file, source)?;
    let mut compiler = Compiler {
        file,
        tokens,
        cursor: 0,
        code: Vec::new(),
        numbers: Vec::new(),
        strings: Vec::new(),
        functions: Vec::new(),
        hosts: Vec::new(),
        signatures: HashMap::new(),
        scopes: Vec::new(),
        locals: 0,
        max_locals: 0,
        loops: Vec::new(),
    };
    compiler.collect_signatures()?;
    while !matches!(compiler.peek(), Tok::Eof) {
        compiler.function()?;
    }

    let mut hooks: Vec<ScriptHookEntry> = compiler
        .functions
        .iter()
        .enumerate()
        .filter_map(|(index, function)| {
            ScriptHook::from_name(&function.name).map(|hook| ScriptHookEntry {
                hook,
                function: u32::try_from(index).unwrap_or(0),
            })
        })
        .collect();
    hooks.sort_by_key(|entry| entry.hook.as_str());

    if hooks.is_empty() {
        return Err(ScriptFault::new(
            file,
            1,
            1,
            "This script defines no lifecycle hook, so nothing would ever call it.",
            "Add at least one of `on_start()`, `on_update(dt)`, `on_collision(other)` or `on_trigger(other)`.",
        ));
    }

    Ok(ScriptProgram {
        file: file.to_owned(),
        code: compiler.code,
        numbers: compiler.numbers,
        strings: compiler.strings,
        functions: compiler.functions,
        hosts: compiler.hosts,
        hooks,
        step_budget: SCRIPT_STEP_BUDGET,
        call_depth: SCRIPT_CALL_DEPTH,
    })
}

/// The host surface as text — one source of truth for the model prompt and the script
/// editor's help, generated from `HOST_FNS` so it cannot drift from what compiles.
#[must_use]
pub fn host_reference() -> String {
    let mut out = String::from(
        "Script subset (ADR-0030): fn, let, assignment, if/else if/else, while, return, break,\n\
         continue, arithmetic, comparison, && and || (short-circuit), number/string/bool\n\
         literals, calls. No closures, arrays, maps, objects, for, switch or imports.\n\n\
         Lifecycle hooks: on_start(), on_update(dt), on_collision(other), on_trigger(other).\n\n\
         Host functions:\n",
    );
    for entry in HOST_FNS {
        let params = vec!["_"; entry.arity].join(", ");
        out.push_str(&format!("  {}({params}) — {}\n", entry.name, entry.doc));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{compile, host_fn, host_reference, OpCode, ScriptHook, HOST_FNS};

    const SAMPLE: &str = r#"
        // A door that opens when the player is close and holds a key.
        fn on_start() {
            set_var("door.open", false);
        }

        fn on_update(dt) {
            let player = find("Player");
            if player == "" { return; }
            let near = distance(self_id(), player) < 2.5;
            if near && get_var("player.keys") > 0 {
                set_var("door.open", true);
                translate(self_id(), 0.0, dt * 2.0, 0.0);
            }
        }
    "#;

    #[test]
    fn a_realistic_script_compiles_with_its_hooks() {
        let program = compile("assets/scripts/door.rhai", SAMPLE).expect("compiles");
        assert_eq!(program.hook_names(), vec!["on_start", "on_update"]);
        assert_eq!(program.functions.len(), 2);
        assert!(program.strings.contains(&"door.open".to_owned()));
        assert!(program
            .code
            .iter()
            .any(|instr| matches!(instr.op, OpCode::CallHost)));
    }

    #[test]
    fn a_program_names_the_hosts_it_calls_and_indexes_only_those() {
        let program = compile("s.rhai", SAMPLE).expect("compiles");
        assert!(program.hosts.contains(&"set_var".to_owned()));
        assert!(program.hosts.contains(&"find".to_owned()));
        assert!(
            !program.hosts.contains(&"spawn".to_owned()),
            "a program must not carry hosts it never calls"
        );
        for instr in &program.code {
            if matches!(instr.op, OpCode::CallHost) {
                let name = &program.hosts[usize::try_from(instr.a).expect("in range")];
                let (_, host) = host_fn(name).expect("every named host is real");
                assert_eq!(i32::try_from(host.arity).expect("small"), instr.b);
            }
        }
    }

    #[test]
    fn every_instruction_carries_a_line_so_a_fault_is_located() {
        let program = compile("s.rhai", SAMPLE).expect("compiles");
        assert!(program.code.iter().all(|instr| instr.line >= 1));
        let max = program
            .code
            .iter()
            .map(|instr| instr.line)
            .max()
            .unwrap_or(0);
        assert!(max > 5, "lines must track the source, got {max}");
    }

    #[test]
    fn an_unsupported_construct_is_named_not_merely_rejected() {
        let fault = compile("s.rhai", "fn on_start() { for x in 0..3 { log(\"hi\"); } }")
            .expect_err("for is outside the subset");
        assert!(fault.message.contains("for"), "{}", fault.message);
        assert!(fault.hint.as_deref().unwrap_or_default().contains("while"));
        assert_eq!(fault.line, 1);
    }

    #[test]
    fn a_misspelled_host_call_suggests_the_real_one() {
        let fault =
            compile("s.rhai", "fn on_start() { set_va(\"a\", 1); }").expect_err("no such host fn");
        assert!(fault
            .hint
            .as_deref()
            .unwrap_or_default()
            .contains("set_var"));
    }

    #[test]
    fn wrong_arity_is_caught_at_compile_time_with_the_docs() {
        let fault =
            compile("s.rhai", "fn on_start() { set_var(\"a\"); }").expect_err("set_var takes two");
        assert!(fault.message.contains("2 argument"), "{}", fault.message);
    }

    #[test]
    fn a_hook_declared_with_the_wrong_shape_is_rejected() {
        let fault =
            compile("s.rhai", "fn on_update() { log(\"tick\"); }").expect_err("on_update takes dt");
        assert!(fault.message.contains("on_update"));
        assert!(fault.hint.as_deref().unwrap_or_default().contains("dt"));
    }

    #[test]
    fn a_file_with_no_hook_is_rejected_rather_than_silently_never_running() {
        let fault = compile("s.rhai", "fn helper() { log(\"x\"); }").expect_err("no hook");
        assert!(fault.message.contains("no lifecycle hook"));
    }

    #[test]
    fn functions_may_call_each_other_in_either_order() {
        let program = compile(
            "s.rhai",
            "fn on_start() { log(to_string(twice(3.0))); } fn twice(v) { return v * 2.0; }",
        )
        .expect("mutual visibility");
        assert!(program
            .code
            .iter()
            .any(|instr| matches!(instr.op, OpCode::CallUser)));
    }

    #[test]
    fn an_undeclared_variable_is_a_located_fault() {
        let fault =
            compile("s.rhai", "fn on_start() {\n  log(missing);\n}").expect_err("no such local");
        assert_eq!(fault.line, 2);
        assert!(fault.message.contains("missing"));
    }

    #[test]
    fn break_outside_a_loop_is_rejected() {
        let fault = compile("s.rhai", "fn on_start() { break; }").expect_err("no loop");
        assert!(fault.message.contains("while"));
    }

    #[test]
    fn a_while_loop_compiles_with_break_and_continue() {
        let program = compile(
            "s.rhai",
            "fn on_start() { let i = 0.0; while i < 10.0 { i += 1.0; if i == 3.0 { continue; } if i > 6.0 { break; } } }",
        )
        .expect("loops compile");
        assert!(program
            .code
            .iter()
            .any(|instr| matches!(instr.op, OpCode::Jump)));
    }

    #[test]
    fn short_circuit_operators_emit_peeking_jumps() {
        let program = compile(
            "s.rhai",
            "fn on_start() { if true && false { log(\"x\"); } }",
        )
        .expect("compiles");
        assert!(program
            .code
            .iter()
            .any(|instr| matches!(instr.op, OpCode::JumpIfFalsePeek)));
    }

    #[test]
    fn host_names_are_unique_and_documented() {
        let mut seen = std::collections::BTreeSet::new();
        for entry in HOST_FNS {
            assert!(seen.insert(entry.name), "duplicate host fn {}", entry.name);
            assert!(!entry.doc.is_empty(), "{} has no doc", entry.name);
            assert!(entry.arity <= 4, "{} takes too many arguments", entry.name);
        }
        assert!(host_fn("set_var").is_some());
        assert!(host_fn("eval").is_none(), "eval must never be a host call");
    }

    #[test]
    fn the_host_reference_lists_every_function() {
        let text = host_reference();
        for entry in HOST_FNS {
            assert!(
                text.contains(entry.name),
                "{} missing from reference",
                entry.name
            );
        }
    }

    #[test]
    fn hook_arities_match_what_the_runtime_calls() {
        assert_eq!(ScriptHook::OnStart.arity(), 0);
        assert_eq!(ScriptHook::OnUpdate.arity(), 1);
        assert_eq!(
            ScriptHook::from_name("on_trigger"),
            Some(ScriptHook::OnTrigger)
        );
        assert_eq!(ScriptHook::from_name("on_render"), None);
    }

    #[test]
    fn an_unterminated_string_is_reported_where_it_starts() {
        let fault = compile("s.rhai", "fn on_start() { log(\"oops); }").expect_err("unclosed");
        assert!(fault.message.contains("never closed"));
        assert_eq!(fault.line, 1);
    }

    #[test]
    fn redefining_a_host_function_is_refused() {
        let fault = compile("s.rhai", "fn log(x) { return x; } fn on_start() { }")
            .expect_err("log is a host fn");
        assert!(fault.message.contains("built-in"));
    }

    #[test]
    fn a_budget_and_a_depth_cap_travel_with_every_program() {
        let program = compile("s.rhai", "fn on_start() { log(\"x\"); }").expect("compiles");
        assert_eq!(program.step_budget, super::SCRIPT_STEP_BUDGET);
        assert_eq!(program.call_depth, super::SCRIPT_CALL_DEPTH);
    }

    #[test]
    fn nested_scopes_do_not_collide_on_slots() {
        let program = compile(
            "s.rhai",
            "fn on_start() { let a = 1.0; if true { let b = 2.0; log(to_string(a + b)); } let c = 3.0; log(to_string(c)); }",
        )
        .expect("compiles");
        let function = &program.functions[0];
        // The frame is sized to the high-water mark, not the declaration count: `b` leaves
        // scope at the closing brace, so `c` takes its slot back. Two slots, three names.
        assert_eq!(function.locals, 2);
    }
}
