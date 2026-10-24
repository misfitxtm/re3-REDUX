"""Tiny C preprocessor subset: #if/#ifdef/#elif/#else/#endif with integer defines."""
import re


def make(DEFINES):
    def strip_comments(t):
        t = re.sub(r"/\*.*?\*/", "", t, flags=re.S)
        return re.sub(r"//[^\n]*", "", t)


    def ev(expr):
        expr = re.sub(r"defined\s*\(\s*(\w+)\s*\)", lambda m: "1" if m.group(1) in DEFINES else "0", expr)
        expr = re.sub(r"[A-Za-z_]\w*", lambda m: str(DEFINES.get(m.group(0), 0)), expr)
        expr = expr.replace("&&", " and ").replace("||", " or ")
        expr = re.sub(r"!(?!=)", " not ", expr)
        return bool(eval(expr))


    def preprocess(t):
        out, stack = [], []  # stack of (active, taken, parent_active)
        active = True
        for line in strip_comments(t).split("\n"):
            m = re.match(r"\s*#\s*(\w+)\s*(.*)", line)
            if not m or m.group(1) not in ("if", "ifdef", "ifndef", "elif", "else", "endif"):
                if active:
                    out.append(line)
                continue
            d, arg = m.group(1), m.group(2).strip()
            if d in ("if", "ifdef", "ifndef"):
                c = ev(arg) if d == "if" else ((arg in DEFINES) if d == "ifdef" else (arg not in DEFINES))
                stack.append((active, c))
                active = active and c
            elif d == "elif":
                parent, taken = stack.pop()
                c = parent and not taken and ev(arg)
                stack.append((parent, taken or c))
                active = c
            elif d == "else":
                parent, taken = stack.pop()
                stack.append((parent, True))
                active = parent and not taken
            else:
                parent, _ = stack.pop()
                active = parent
        return "\n".join(out)


    return preprocess, strip_comments
