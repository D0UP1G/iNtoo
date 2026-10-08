"""Source preserving lexer and active-node reader for Niri's KDL config."""

from dataclasses import dataclass
import json


@dataclass(frozen=True)
class Token:
    kind: str
    value: str
    start: int
    end: int
    disabled: bool = False


@dataclass(frozen=True)
class Node:
    name: str
    args: tuple
    start: int
    end: int
    disabled: bool = False


def _decode_string(body):
    result = []
    i = 0
    simple = {"n": "\n", "r": "\r", "t": "\t", "b": "\b", "f": "\f",
              "\\": "\\", '"': '"', "/": "/"}
    while i < len(body):
        if body[i] != "\\":
            result.append(body[i]); i += 1; continue
        i += 1
        if i >= len(body): raise ValueError("unfinished KDL escape")
        esc = body[i]
        if esc in simple:
            result.append(simple[esc]); i += 1
        elif esc == "u" and i + 1 < len(body) and body[i + 1] == "{":
            end = body.find("}", i + 2)
            if end < 0: raise ValueError("unfinished KDL Unicode escape")
            result.append(chr(int(body[i + 2:end], 16))); i = end + 1
        elif esc in ("u", "U"):
            width = 4 if esc == "u" else 8
            result.append(chr(int(body[i + 1:i + 1 + width], 16))); i += width + 1
        elif esc == "\n":
            i += 1
            while i < len(body) and body[i] in " \t": i += 1
        else:
            raise ValueError(f"unsupported KDL escape: \\{esc}")
    return "".join(result)


def _read_string(text, start):
    i = start
    raw = False
    hashes = 0
    if text[i] == "r":
        raw = True; i += 1
        while i < len(text) and text[i] == "#": hashes += 1; i += 1
        if i >= len(text) or text[i] != '"': return None
    if text[i] != '"': return None
    triple = not raw and text.startswith('"""', i)
    opener = '"""' if triple else '"'
    content = i + len(opener)
    closer = opener + ("#" * hashes if raw else "")
    cursor = content
    escaped = False
    while cursor < len(text):
        if raw or triple:
            if text.startswith(closer, cursor):
                end = cursor + len(closer)
                value = text[content:cursor] if raw or triple else _decode_string(text[content:cursor])
                return Token("string", value, start, end)
        else:
            c = text[cursor]
            if escaped: escaped = False
            elif c == "\\": escaped = True
            elif c == '"':
                body = text[content:cursor]
                try: value = json.loads('"' + body + '"')
                except (json.JSONDecodeError, ValueError): value = _decode_string(body)
                return Token("string", value, start, cursor + 1)
        cursor += 1
    raise ValueError("unterminated KDL string")


def lex(text):
    tokens = []
    i = 0
    while i < len(text):
        c = text[i]
        if c in " \t\r": i += 1; continue
        if c == "\\":
            j = i + 1
            while j < len(text) and text[j] in " \t": j += 1
            if text.startswith("//", j):
                j = text.find("\n", j)
                if j < 0: i = len(text); continue
            if j < len(text) and text[j] == "\n":
                i = j + 1
                while i < len(text) and text[i] in " \t": i += 1
                continue
            if text.startswith("\r\n", j): i = j + 2; continue
        if c == "\n": tokens.append(Token("newline", c, i, i + 1)); i += 1; continue
        if text.startswith("//", i):
            j = text.find("\n", i + 2); i = len(text) if j < 0 else j; continue
        if text.startswith("/*", i):
            depth = 1; j = i + 2
            while j < len(text) and depth:
                if text.startswith("/*", j): depth += 1; j += 2
                elif text.startswith("*/", j): depth -= 1; j += 2
                else:
                    j += 1
            if depth: raise ValueError("unterminated KDL block comment")
            i = j; continue
        if text.startswith("/-", i):
            tokens.append(Token("disable", "/-", i, i + 2)); i += 2; continue
        string = _read_string(text, i)
        if string is not None: tokens.append(string); i = string.end; continue
        if c in "{};=": tokens.append(Token(c, c, i, i + 1)); i += 1; continue
        start = i
        while i < len(text) and text[i] not in " \t\r\n{};=\"":
            if text.startswith(("//", "/*"), i) or text.startswith("/-", i): break
            i += 1
        if i == start: i += 1; continue
        tokens.append(Token("word", text[start:i], start, i))
    return tokens


def nodes(text):
    """Yield node headers, excluding slash-dash nodes and their subtrees."""
    tokens = lex(text)
    i = 0
    disabled_depth = 0
    while i < len(tokens):
        token = tokens[i]
        if token.kind == "}":
            disabled_depth = max(0, disabled_depth - 1); i += 1; continue
        if token.kind in ("newline", ";", "{"):
            i += 1; continue

        disabled = disabled_depth > 0
        node_marker = token.kind == "disable"
        if node_marker:
            i += 1
            while i < len(tokens) and tokens[i].kind == "newline": i += 1
            if i >= len(tokens): break
            token = tokens[i]
            disabled = True

        if token.kind not in ("word", "string"):
            i += 1; continue
        name = token.value
        start = token.start
        args = []
        i += 1
        pending_arg_disabled = False
        while i < len(tokens):
            cur = tokens[i]
            if cur.kind in ("newline", ";", "{", "}"):
                break
            if cur.kind == "disable":
                pending_arg_disabled = True; i += 1; continue
            args.append(Token(cur.kind, cur.value, cur.start, cur.end, pending_arg_disabled))
            pending_arg_disabled = False
            i += 1
        end = tokens[i - 1].end if i > 0 else token.end
        node = Node(name, tuple(args), start, end, disabled)
        yield node
        # A slash-dash node disables its whole child block. A normal node's
        # opening brace also enters a block, so preserve current disable state.
        if i < len(tokens) and tokens[i].kind == "{":
            # In forms such as `binds /- { ... }`, slash-dash precedes the
            # child block rather than a scalar argument. The whole subtree is
            # disabled even though the parent node itself remains active.
            if disabled or pending_arg_disabled:
                disabled_depth += 1
            i += 1
        elif i < len(tokens) and tokens[i].kind == ";":
            i += 1


def string_arguments(node):
    return [token.value for token in node.args if token.kind == "string" and not token.disabled]


def include_path(node):
    """Return the first active positional string argument of an include."""
    if node.name != "include" or node.disabled:
        return None
    for token in node.args:
        if token.disabled:
            continue
        if token.kind == "string":
            return token.value
        # A positional path must precede properties; no supported path exists.
        if token.kind == "word" and "=" not in token.value:
            return None
    return None


def mask_comments(text):
    chars = list(text)
    i = 0
    while i < len(text):
        string = _read_string(text, i) if text[i] in ('"', "r") else None
        if string: i = string.end; continue
        if text.startswith("//", i):
            while i < len(text) and text[i] != "\n": chars[i] = " "; i += 1
        elif text.startswith("/*", i):
            depth = 1; chars[i:i + 2] = [" ", " "]; i += 2
            while i < len(text) and depth:
                if text.startswith("/*", i): depth += 1; chars[i:i + 2] = [" ", " "]; i += 2
                elif text.startswith("*/", i): depth -= 1; chars[i:i + 2] = [" ", " "]; i += 2
                else:
                    if text[i] not in "\r\n": chars[i] = " "
                    i += 1
        else: i += 1
    return "".join(chars)


def clear_span(text, start, end):
    return "".join("\n" if c == "\n" else "\r" if c == "\r" else " " for c in text[start:end])
