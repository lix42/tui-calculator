//! Expression parser and evaluator.
//!
//! Recursive descent grammar:
//!   expr   = term (('+' | '-') term)*
//!   term   = factor (('*' | '/') factor)*
//!   factor = '-' factor | '(' expr ')' | number
//!   number = [0-9]+ ('.' [0-9]+)?

/// A committed unit of an expression. `App` stores its expression as a
/// `Vec<Token>` (internal truth) rather than a string, so computed values keep
/// full `f64` precision instead of round-tripping through a truncated display
/// string. `eval_tokens` evaluates this representation directly.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Token {
    Number(f64), // a committed value (typed or computed)
    Op(char),    // '+', '-', '*', '/'
    LParen,      // '('
    RParen,      // ')'
}

/// A cursor over a token slice. Tokens are `Copy`, so `peek`/`advance` hand
/// back owned values.
struct TokenCursor<'a> {
    tokens: &'a [Token],
    pos: usize,
}

impl<'a> TokenCursor<'a> {
    fn new(tokens: &'a [Token]) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> Option<Token> {
        self.tokens.get(self.pos).copied()
    }

    fn advance(&mut self) -> Option<Token> {
        let t = self.peek()?;
        self.pos += 1;
        Some(t)
    }
}

/// Public entry point: evaluate a committed expression to an `f64`. Consumes
/// `&[Token]` directly so the caller never has to format computed values back
/// into a string.
pub fn eval_tokens(tokens: &[Token]) -> Result<f64, String> {
    let mut c = TokenCursor::new(tokens);
    let value = parse_expr(&mut c)?;
    if let Some(t) = c.peek() {
        return Err(format!("unexpected token {:?}", t));
    }
    Ok(value)
}

/// expr = term (('+' | '-') term)*
fn parse_expr(c: &mut TokenCursor) -> Result<f64, String> {
    let mut lhs = parse_term(c)?;
    loop {
        match c.peek() {
            Some(Token::Op('+')) => {
                c.advance();
                lhs += parse_term(c)?;
            }
            Some(Token::Op('-')) => {
                c.advance();
                lhs -= parse_term(c)?;
            }
            _ => return Ok(lhs),
        }
    }
}

/// term = factor (('*' | '/') factor)*
fn parse_term(c: &mut TokenCursor) -> Result<f64, String> {
    let mut lhs = parse_factor(c)?;
    loop {
        match c.peek() {
            Some(Token::Op('*')) => {
                c.advance();
                lhs *= parse_factor(c)?;
            }
            Some(Token::Op('/')) => {
                c.advance();
                let rhs = parse_factor(c)?;
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
fn parse_factor(c: &mut TokenCursor) -> Result<f64, String> {
    match c.peek() {
        Some(Token::Op('-')) => {
            c.advance();
            Ok(-parse_factor(c)?)
        }
        Some(Token::LParen) => {
            c.advance();
            let value = parse_expr(c)?;
            match c.advance() {
                Some(Token::RParen) => Ok(value),
                Some(t) => Err(format!("expected ')', got {:?}", t)),
                None => Err("expected ')', got end of input".to_string()),
            }
        }
        Some(Token::Number(n)) => {
            c.advance();
            Ok(n)
        }
        Some(t) => Err(format!("unexpected token {:?}", t)),
        None => Err("unexpected end of input".to_string()),
    }
}

#[cfg(test)]
mod token_tests {
    use super::Token::*;
    use super::eval_tokens;

    #[test]
    fn simple_addition() {
        assert_eq!(
            eval_tokens(&[Number(1.0), Op('+'), Number(2.0)]).unwrap(),
            3.0
        );
    }

    #[test]
    fn precedence() {
        // 78 - 65 * 5 = -247
        let tokens = [Number(78.0), Op('-'), Number(65.0), Op('*'), Number(5.0)];
        assert_eq!(eval_tokens(&tokens).unwrap(), -247.0);
    }

    #[test]
    fn parens_override_precedence() {
        // (1 + 2) * 3 = 9
        let tokens = [
            LParen,
            Number(1.0),
            Op('+'),
            Number(2.0),
            RParen,
            Op('*'),
            Number(3.0),
        ];
        assert_eq!(eval_tokens(&tokens).unwrap(), 9.0);
    }

    #[test]
    fn unary_minus() {
        assert_eq!(
            eval_tokens(&[Op('-'), Number(5.0), Op('+'), Number(3.0)]).unwrap(),
            -2.0
        );
        // --5 = 5
        assert_eq!(eval_tokens(&[Op('-'), Op('-'), Number(5.0)]).unwrap(), 5.0);
    }

    #[test]
    fn division_by_zero_is_an_error() {
        assert!(eval_tokens(&[Number(1.0), Op('/'), Number(0.0)]).is_err());
    }

    #[test]
    fn unmatched_paren_is_an_error() {
        assert!(eval_tokens(&[LParen, Number(1.0), Op('+'), Number(2.0)]).is_err());
    }

    #[test]
    fn trailing_operator_is_an_error() {
        // e.g. pressing `=` on `78 -` — factor sees end-of-input.
        assert!(eval_tokens(&[Number(78.0), Op('-')]).is_err());
    }
}
