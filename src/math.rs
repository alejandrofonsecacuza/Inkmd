//! Best-effort conversion of TeX math to readable Unicode text.
//!
//! This is not a TeX engine: it maps common commands to symbols, turns
//! simple super/subscripts into Unicode where possible and flattens the
//! rest, so `$\alpha^2 + \frac{1}{n}$` reads as `α² + 1/n`. Anything it
//! does not understand is kept verbatim.

pub fn to_unicode(src: &str) -> String {
    let mut p = Parser { s: src.as_bytes(), src, i: 0 };
    let mut out = String::with_capacity(src.len());
    p.seq(&mut out, None);
    // Collapse runs of whitespace produced by spacing commands.
    let mut res = String::with_capacity(out.len());
    let mut prev_space = false;
    for c in out.trim().chars() {
        if c == ' ' {
            if !prev_space {
                res.push(' ');
            }
            prev_space = true;
        } else {
            res.push(c);
            prev_space = false;
        }
    }
    res
}

struct Parser<'a> {
    s: &'a [u8],
    src: &'a str,
    i: usize,
}

impl Parser<'_> {
    fn seq(&mut self, out: &mut String, stop: Option<u8>) {
        while self.i < self.s.len() {
            let c = self.s[self.i];
            if Some(c) == stop {
                self.i += 1;
                return;
            }
            self.atom(out);
        }
    }

    /// Parse one group/command/char into `out`.
    fn atom(&mut self, out: &mut String) {
        let c = self.s[self.i];
        match c {
            b'{' => {
                self.i += 1;
                self.seq(out, Some(b'}'));
            }
            b'\\' => self.command(out),
            b'^' | b'_' => {
                self.i += 1;
                let mut arg = String::new();
                self.skip_ws();
                if self.i < self.s.len() {
                    self.atom(&mut arg);
                }
                let map = if c == b'^' { superscript } else { subscript };
                match arg.chars().map(map).collect::<Option<String>>() {
                    Some(conv) if !arg.is_empty() => out.push_str(&conv),
                    _ => {
                        out.push(c as char);
                        if arg.chars().count() > 1 {
                            out.push('(');
                            out.push_str(&arg);
                            out.push(')');
                        } else {
                            out.push_str(&arg);
                        }
                    }
                }
            }
            b'~' => {
                self.i += 1;
                out.push(' ');
            }
            b'&' => {
                self.i += 1;
                out.push(' ');
            }
            b'\n' | b'\t' => {
                self.i += 1;
                out.push(' ');
            }
            _ => {
                let ch = self.src[self.i..].chars().next().unwrap();
                self.i += ch.len_utf8();
                out.push(ch);
            }
        }
    }

    fn skip_ws(&mut self) {
        while self.i < self.s.len() && self.s[self.i] == b' ' {
            self.i += 1;
        }
    }

    fn arg(&mut self) -> String {
        self.skip_ws();
        let mut a = String::new();
        if self.i < self.s.len() {
            self.atom(&mut a);
        }
        a
    }

    fn command(&mut self, out: &mut String) {
        self.i += 1; // backslash
        let start = self.i;
        while self.i < self.s.len() && self.s[self.i].is_ascii_alphabetic() {
            self.i += 1;
        }
        if self.i == start {
            // Control symbol: \\, \,, \{, \% ...
            if self.i < self.s.len() {
                let ch = self.src[self.i..].chars().next().unwrap();
                self.i += ch.len_utf8();
                match ch {
                    '\\' => out.push('\n'),
                    ',' | ';' | ':' | '!' | ' ' => out.push(' '),
                    other => out.push(other),
                }
            }
            return;
        }
        let name = &self.src[start..self.i];
        match name {
            "frac" | "dfrac" | "tfrac" => {
                let a = self.arg();
                let b = self.arg();
                out.push_str(&wrap_if_complex(&a));
                out.push('/');
                out.push_str(&wrap_if_complex(&b));
            }
            "sqrt" => {
                let a = self.arg();
                out.push('√');
                out.push_str(&wrap_if_complex(&a));
            }
            "text" | "mathrm" | "mathbf" | "mathit" | "mathsf" | "mathtt" | "textbf" | "textit" | "operatorname"
            | "boldsymbol" | "mathcal" | "mathbb" | "mathfrak" | "displaystyle" | "left" | "right" | "big" | "Big"
            | "bigg" | "Bigg" => {
                if matches!(name, "left" | "right" | "big" | "Big" | "bigg" | "Bigg" | "displaystyle") {
                    // These just size the next delimiter.
                    if name != "displaystyle" && self.i < self.s.len() && self.s[self.i] == b'.' {
                        self.i += 1;
                    }
                    return;
                }
                let a = self.arg();
                if name == "mathbb" {
                    out.extend(a.chars().map(double_struck));
                } else {
                    out.push_str(&a);
                }
            }
            "begin" | "end" => {
                // \begin{matrix} ... — drop the environment name.
                let _ = self.arg();
                out.push(' ');
            }
            "quad" | "qquad" => out.push_str("  "),
            _ => match symbol(name) {
                Some(sym) => {
                    out.push_str(sym);
                    // Word-like operators need a separating space.
                    if sym.chars().all(|c| c.is_ascii_alphabetic()) {
                        out.push(' ');
                    }
                }
                None => {
                    out.push('\\');
                    out.push_str(name);
                }
            },
        }
    }
}

fn wrap_if_complex(s: &str) -> String {
    if s.chars().count() <= 1 || s.chars().all(|c| c.is_alphanumeric() || c == '.') {
        s.to_string()
    } else {
        format!("({s})")
    }
}

fn superscript(c: char) -> Option<char> {
    Some(match c {
        '0' => '⁰',
        '1' => '¹',
        '2' => '²',
        '3' => '³',
        '4' => '⁴',
        '5' => '⁵',
        '6' => '⁶',
        '7' => '⁷',
        '8' => '⁸',
        '9' => '⁹',
        '+' => '⁺',
        '-' | '−' => '⁻',
        '=' => '⁼',
        '(' => '⁽',
        ')' => '⁾',
        'n' => 'ⁿ',
        'i' => 'ⁱ',
        'T' => 'ᵀ',
        '′' => '′',
        _ => return None,
    })
}

fn subscript(c: char) -> Option<char> {
    Some(match c {
        '0' => '₀',
        '1' => '₁',
        '2' => '₂',
        '3' => '₃',
        '4' => '₄',
        '5' => '₅',
        '6' => '₆',
        '7' => '₇',
        '8' => '₈',
        '9' => '₉',
        '+' => '₊',
        '-' => '₋',
        '=' => '₌',
        '(' => '₍',
        ')' => '₎',
        'a' => 'ₐ',
        'e' => 'ₑ',
        'i' => 'ᵢ',
        'j' => 'ⱼ',
        'k' => 'ₖ',
        'n' => 'ₙ',
        'm' => 'ₘ',
        'o' => 'ₒ',
        'x' => 'ₓ',
        't' => 'ₜ',
        _ => return None,
    })
}

fn double_struck(c: char) -> char {
    match c {
        'R' => 'ℝ',
        'N' => 'ℕ',
        'Z' => 'ℤ',
        'Q' => 'ℚ',
        'C' => 'ℂ',
        'P' => 'ℙ',
        other => other,
    }
}

fn symbol(name: &str) -> Option<&'static str> {
    Some(match name {
        "alpha" => "α",
        "beta" => "β",
        "gamma" => "γ",
        "delta" => "δ",
        "epsilon" | "varepsilon" => "ε",
        "zeta" => "ζ",
        "eta" => "η",
        "theta" | "vartheta" => "θ",
        "iota" => "ι",
        "kappa" => "κ",
        "lambda" => "λ",
        "mu" => "μ",
        "nu" => "ν",
        "xi" => "ξ",
        "pi" => "π",
        "rho" | "varrho" => "ρ",
        "sigma" => "σ",
        "tau" => "τ",
        "upsilon" => "υ",
        "phi" | "varphi" => "φ",
        "chi" => "χ",
        "psi" => "ψ",
        "omega" => "ω",
        "Gamma" => "Γ",
        "Delta" => "Δ",
        "Theta" => "Θ",
        "Lambda" => "Λ",
        "Xi" => "Ξ",
        "Pi" => "Π",
        "Sigma" => "Σ",
        "Phi" => "Φ",
        "Psi" => "Ψ",
        "Omega" => "Ω",
        "times" => "×",
        "cdot" => "·",
        "div" => "÷",
        "pm" => "±",
        "mp" => "∓",
        "leq" | "le" => "≤",
        "geq" | "ge" => "≥",
        "neq" | "ne" => "≠",
        "approx" => "≈",
        "equiv" => "≡",
        "sim" => "∼",
        "simeq" => "≃",
        "propto" => "∝",
        "infty" => "∞",
        "sum" => "∑",
        "prod" => "∏",
        "int" => "∫",
        "iint" => "∬",
        "oint" => "∮",
        "partial" => "∂",
        "nabla" => "∇",
        "in" => "∈",
        "notin" => "∉",
        "ni" => "∋",
        "subset" => "⊂",
        "subseteq" => "⊆",
        "supset" => "⊃",
        "supseteq" => "⊇",
        "cup" => "∪",
        "cap" => "∩",
        "emptyset" | "varnothing" => "∅",
        "forall" => "∀",
        "exists" => "∃",
        "neg" | "lnot" => "¬",
        "land" | "wedge" => "∧",
        "lor" | "vee" => "∨",
        "to" | "rightarrow" => "→",
        "leftarrow" | "gets" => "←",
        "Rightarrow" | "implies" => "⇒",
        "Leftarrow" => "⇐",
        "leftrightarrow" => "↔",
        "Leftrightarrow" | "iff" => "⇔",
        "mapsto" => "↦",
        "uparrow" => "↑",
        "downarrow" => "↓",
        "cdots" | "dots" | "ldots" => "…",
        "vdots" => "⋮",
        "ddots" => "⋱",
        "prime" => "′",
        "circ" => "∘",
        "bullet" => "•",
        "star" => "⋆",
        "oplus" => "⊕",
        "otimes" => "⊗",
        "langle" => "⟨",
        "rangle" => "⟩",
        "lceil" => "⌈",
        "rceil" => "⌉",
        "lfloor" => "⌊",
        "rfloor" => "⌋",
        "mid" => "|",
        "hbar" => "ℏ",
        "ell" => "ℓ",
        "Re" => "ℜ",
        "Im" => "ℑ",
        "aleph" => "ℵ",
        "angle" => "∠",
        "perp" => "⊥",
        "parallel" => "∥",
        "therefore" => "∴",
        "because" => "∵",
        "sin" => "sin",
        "cos" => "cos",
        "tan" => "tan",
        "log" => "log",
        "ln" => "ln",
        "exp" => "exp",
        "lim" => "lim",
        "max" => "max",
        "min" => "min",
        "det" => "det",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::to_unicode;

    #[test]
    fn converts_common_tex() {
        assert_eq!(to_unicode(r"\alpha^2 + \beta_1"), "α² + β₁");
        assert_eq!(to_unicode(r"\frac{1}{n}"), "1/n");
        assert_eq!(to_unicode(r"\frac{a+b}{2}"), "(a+b)/2");
        assert_eq!(to_unicode(r"\sqrt{x^2+y^2}"), "√(x²+y²)");
        assert_eq!(to_unicode(r"\sum_{i=1}^{n} i"), "∑ᵢ₌₁ⁿ i");
        assert_eq!(to_unicode(r"x^{ab}"), "x^(ab)");
        assert_eq!(to_unicode(r"\mathbb{R}^n \to \mathbb{R}"), "ℝⁿ → ℝ");
        assert_eq!(to_unicode(r"\unknown{x}"), r"\unknownx");
        assert_eq!(to_unicode(r"\sin x"), "sin x");
    }
}
