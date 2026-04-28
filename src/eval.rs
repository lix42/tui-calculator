//! Expression parser and evaluator.
//!
//! Recursive descent grammar:
//!   expr   = term (('+' | '-') term)*
//!   term   = factor (('*' | '/') factor)*
//!   factor = '-' factor | '(' expr ')' | number
//!   number = [0-9]+ ('.' [0-9]+)?

/// A small cursor over the input string. We collect into `Vec<char>` so we
/// can peek/advance by index — simpler than juggling a `Peekable<Chars>`.
struct Parser {
    chars: Vec<char>,
    pos: usize,
}

impl Parser {
    fn new(input: &str) -> Self {
        Self {
            chars: input.chars().collect(),
            pos: 0,
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += 1;
        Some(c)
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(c) if c.is_whitespace()) {
            self.pos += 1;
        }
    }
}

/// Public entry point: evaluate an arithmetic expression to an `f64`.
pub fn eval(input: &str) -> Result<f64, String> {
    let mut p = Parser::new(input);
    let value = parse_expr(&mut p)?;
    p.skip_whitespace();
    if let Some(c) = p.peek() {
        return Err(format!(
            "unexpected character '{}' at position {}",
            c, p.pos
        ));
    }
    Ok(value)
}

/// expr = term (('+' | '-') term)*
fn parse_expr(p: &mut Parser) -> Result<f64, String> {
    let mut lhs = parse_term(p)?;
    loop {
        p.skip_whitespace();
        match p.peek() {
            Some('+') => {
                p.advance();
                lhs += parse_term(p)?;
            }
            Some('-') => {
                p.advance();
                lhs -= parse_term(p)?;
            }
            _ => return Ok(lhs),
        }
    }
}

/// term = factor (('*' | '/') factor)*
fn parse_term(p: &mut Parser) -> Result<f64, String> {
    let mut lhs = parse_factor(p)?;
    loop {
        p.skip_whitespace();
        match p.peek() {
            Some('*') => {
                p.advance();
                lhs *= parse_factor(p)?;
            }
            Some('/') => {
                p.advance();
                let rhs = parse_factor(p)?;
                if rhs == 0.0 {
                    return Err("division by zero".to_string());
                }
                lhs /= rhs;
            }
            _ => return Ok(lhs),
        }
    }
}

/// factor = '-' factor | '(' expr ')' | number
fn parse_factor(p: &mut Parser) -> Result<f64, String> {
    p.skip_whitespace();
    match p.peek() {
        Some('-') => {
            p.advance();
            Ok(-parse_factor(p)?)
        }
        Some('(') => {
            p.advance();
            let value = parse_expr(p)?;
            p.skip_whitespace();
            match p.advance() {
                Some(')') => Ok(value),
                Some(c) => Err(format!("expected ')', got '{}'", c)),
                None => Err("expected ')', got end of input".to_string()),
            }
        }
        Some(c) if c.is_ascii_digit() || c == '.' => parse_number(p),
        Some(c) => Err(format!("unexpected character '{}'", c)),
        None => Err("unexpected end of input".to_string()),
    }
}

/// number = [0-9]+ ('.' [0-9]+)?
fn parse_number(p: &mut Parser) -> Result<f64, String> {
    let start = p.pos;
    while matches!(p.peek(), Some(c) if c.is_ascii_digit()) {
        p.advance();
    }
    if p.peek() == Some('.') {
        p.advance();
        while matches!(p.peek(), Some(c) if c.is_ascii_digit()) {
            p.advance();
        }
    }
    let text: String = p.chars[start..p.pos].iter().collect();
    text.parse::<f64>()
        .map_err(|e| format!("invalid number '{}': {}", text, e))
}

#[cfg(test)]
mod tests {
    use super::eval;

    #[test]
    fn simple_addition() {
        assert_eq!(eval("1+2").unwrap(), 3.0);
    }

    #[test]
    fn precedence() {
        // From the design doc: 78-65*5 = -247
        assert_eq!(eval("78-65*5").unwrap(), -247.0);
    }

    #[test]
    fn parens_override_precedence() {
        assert_eq!(eval("(1+2)*3").unwrap(), 9.0);
    }

    #[test]
    fn unary_minus() {
        assert_eq!(eval("-5+3").unwrap(), -2.0);
        assert_eq!(eval("--5").unwrap(), 5.0);
    }

    #[test]
    fn decimals() {
        assert_eq!(eval("1.5*2").unwrap(), 3.0);
    }

    #[test]
    fn whitespace_is_ignored() {
        assert_eq!(eval(" 1 + 2 * 3 ").unwrap(), 7.0);
    }

    #[test]
    fn division_by_zero_is_an_error() {
        assert!(eval("1/0").is_err());
    }

    #[test]
    fn unmatched_paren_is_an_error() {
        assert!(eval("(1+2").is_err());
    }
}
