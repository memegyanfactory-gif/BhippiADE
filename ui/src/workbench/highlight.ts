/**
 * A small syntax highlighter for the workbench editor.
 *
 * This is deliberately a tokenizer and not a parser. It colours comments, strings,
 * numbers, keywords, types, and function calls — the five distinctions that make code
 * scannable — and gets everything else deliberately wrong-but-harmless by leaving it in
 * the default colour. Shipping a real grammar engine would add megabytes to a desktop
 * bundle for a pane used to glance at a file the agent just wrote.
 *
 * It returns spans rather than an HTML string: nothing here is ever inserted with
 * `innerHTML`, so a file containing `<script>` is text, not markup.
 */

export type Token = { text: string; kind: TokenKind };
export type TokenKind =
  | "plain"
  | "comment"
  | "string"
  | "number"
  | "keyword"
  | "type"
  | "call"
  | "punct"
  | "tag"
  | "attr";

type Grammar = {
  lineComment: string[];
  blockComment: [string, string] | null;
  quotes: string[];
  keywords: Set<string>;
  types: Set<string>;
};

const words = (list: string) => new Set(list.split(" "));

const JS_KEYWORDS =
  "import export from default as const let var function return if else for while do break continue " +
  "switch case new class extends implements interface type enum async await yield try catch finally " +
  "throw typeof instanceof in of delete void this super null undefined true false static get set public " +
  "private protected readonly abstract declare namespace satisfies keyof infer is";

const RUST_KEYWORDS =
  "as async await break const continue crate dyn else enum extern false fn for if impl in let loop match " +
  "mod move mut pub ref return self Self static struct super trait true type unsafe use where while";

const PY_KEYWORDS =
  "and as assert async await break class continue def del elif else except finally for from global if " +
  "import in is lambda None nonlocal not or pass raise return True False try while with yield self";

const COMMON_TYPES =
  "string number boolean bigint symbol object unknown any never void Array Record Promise Map Set Date " +
  "RegExp Error JSON Math u8 u16 u32 u64 usize i8 i16 i32 i64 isize f32 f64 bool str String Vec Option " +
  "Result Box Arc Rc HashMap BTreeMap Some None Ok Err int float bytes dict list tuple";

const GRAMMARS: Record<string, Grammar> = {
  js: {
    lineComment: ["//"],
    blockComment: ["/*", "*/"],
    quotes: ['"', "'", "`"],
    keywords: words(JS_KEYWORDS),
    types: words(COMMON_TYPES),
  },
  rust: {
    lineComment: ["//"],
    blockComment: ["/*", "*/"],
    quotes: ['"'],
    keywords: words(RUST_KEYWORDS),
    types: words(COMMON_TYPES),
  },
  python: {
    lineComment: ["#"],
    blockComment: null,
    quotes: ['"', "'"],
    keywords: words(PY_KEYWORDS),
    types: words(COMMON_TYPES),
  },
  css: {
    lineComment: [],
    blockComment: ["/*", "*/"],
    quotes: ['"', "'"],
    keywords: words("import media supports keyframes from to var calc and not only"),
    types: new Set<string>(),
  },
  data: {
    lineComment: ["#"],
    blockComment: null,
    quotes: ['"', "'"],
    keywords: words("true false null"),
    types: new Set<string>(),
  },
  shell: {
    lineComment: ["#"],
    blockComment: null,
    quotes: ['"', "'"],
    keywords: words("if then else elif fi for while do done case esac function return export local echo cd set"),
    types: new Set<string>(),
  },
};

/** Extension → grammar. Unknown extensions get the plain-text path. */
function grammarFor(language: string): Grammar | null {
  switch (language) {
    case "ts":
    case "tsx":
    case "js":
    case "jsx":
    case "mjs":
    case "cjs":
    case "java":
    case "go":
    case "c":
    case "h":
    case "cpp":
    case "cs":
    case "php":
    case "swift":
    case "kt":
      return GRAMMARS.js;
    case "rs":
      return GRAMMARS.rust;
    case "py":
      return GRAMMARS.python;
    case "css":
    case "scss":
    case "less":
      return GRAMMARS.css;
    case "json":
    case "toml":
    case "yaml":
    case "yml":
      return GRAMMARS.data;
    case "sh":
    case "bash":
    case "zsh":
    case "ps1":
      return GRAMMARS.shell;
    default:
      return null;
  }
}

const IDENT_START = /[A-Za-z_$@#]/;
const IDENT_PART = /[A-Za-z0-9_$]/;
const PUNCT = /[{}()[\].,;:<>=+\-*/%!&|^~?]/;

/**
 * Tokenizes one line, continuing an unterminated block comment from the line before.
 *
 * Per-line tokenizing is what lets the viewer render only the rows on screen: a 5,000
 * line file costs one pass over the visible window, not over the whole buffer. The
 * `inBlock` flag is the only state that has to cross a line boundary.
 */
export function tokenizeLine(
  line: string,
  language: string,
  inBlock: boolean,
): { tokens: Token[]; inBlock: boolean } {
  const grammar = grammarFor(language);
  if (!grammar) return { tokens: [{ text: line, kind: "plain" }], inBlock: false };

  const tokens: Token[] = [];
  let index = 0;
  let block = inBlock;
  let pending = "";

  const flush = () => {
    if (pending) {
      tokens.push({ text: pending, kind: "plain" });
      pending = "";
    }
  };

  while (index < line.length) {
    // An open block comment swallows everything up to its terminator.
    if (block && grammar.blockComment) {
      const end = line.indexOf(grammar.blockComment[1], index);
      if (end === -1) {
        tokens.push({ text: line.slice(index), kind: "comment" });
        return { tokens, inBlock: true };
      }
      tokens.push({ text: line.slice(index, end + 2), kind: "comment" });
      index = end + 2;
      block = false;
      continue;
    }

    const rest = line.slice(index);

    if (grammar.blockComment && rest.startsWith(grammar.blockComment[0])) {
      flush();
      block = true;
      index += 2;
      tokens.push({ text: grammar.blockComment[0], kind: "comment" });
      continue;
    }

    const lineComment = grammar.lineComment.find((marker) => rest.startsWith(marker));
    if (lineComment) {
      flush();
      tokens.push({ text: rest, kind: "comment" });
      return { tokens, inBlock: false };
    }

    const quote = grammar.quotes.find((mark) => rest.startsWith(mark));
    if (quote) {
      flush();
      let cursor = index + 1;
      while (cursor < line.length) {
        if (line[cursor] === "\\") {
          cursor += 2;
          continue;
        }
        if (line[cursor] === quote) {
          cursor += 1;
          break;
        }
        cursor += 1;
      }
      tokens.push({ text: line.slice(index, cursor), kind: "string" });
      index = cursor;
      continue;
    }

    const character = line[index];

    if (/[0-9]/.test(character) && !IDENT_PART.test(line[index - 1] ?? " ")) {
      flush();
      let cursor = index;
      while (cursor < line.length && /[0-9a-fA-FxXoObB._]/.test(line[cursor])) cursor += 1;
      tokens.push({ text: line.slice(index, cursor), kind: "number" });
      index = cursor;
      continue;
    }

    if (IDENT_START.test(character)) {
      flush();
      let cursor = index + 1;
      while (cursor < line.length && IDENT_PART.test(line[cursor])) cursor += 1;
      const word = line.slice(index, cursor);
      // A `(` straight after the identifier means it is being called — the single
      // heuristic here that a real parser would do properly.
      const called = line.slice(cursor).trimStart().startsWith("(");
      const kind: TokenKind = grammar.keywords.has(word)
        ? "keyword"
        : grammar.types.has(word) || /^[A-Z]/.test(word)
          ? "type"
          : called
            ? "call"
            : "plain";
      tokens.push({ text: word, kind });
      index = cursor;
      continue;
    }

    if (PUNCT.test(character)) {
      flush();
      tokens.push({ text: character, kind: "punct" });
      index += 1;
      continue;
    }

    pending += character;
    index += 1;
  }

  flush();
  return { tokens, inBlock: block };
}

/** Tokenizes a whole buffer, carrying block-comment state between lines. */
export function tokenizeLines(lines: string[], language: string): Token[][] {
  let block = false;
  return lines.map((line) => {
    const result = tokenizeLine(line, language, block);
    block = result.inBlock;
    return result.tokens;
  });
}
