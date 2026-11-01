//! 极简算术表达式求值。
//!
//! 组件参数之间经常是派生关系，例如池壁半径 `radius - wall / 2`、堰半径
//! `radius - wall - 0.55`。只支持字面量和参数名的话，这些量只能硬编码，
//! 组件库就没法按尺寸复用。
//!
//! 这里实现递归下降解析，覆盖 `+ - * /`、括号、一元负号和变量引用。
//! 刻意不引第三方库：需求只有四则运算，而候选的 `evalexpr` 是 AGPL-3.0，
//! 对商业分发有传染性；`rhai` 虽是 MIT/Apache 双许可，但作为完整脚本引擎
//! 体积远超需要。

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum ExpressionError {
    UnexpectedCharacter(char),
    UnexpectedEnd,
    UnknownVariable(String),
    DivisionByZero,
    UnbalancedParenthesis,
    NotFinite,
    UnknownFunction(String),
}

impl std::fmt::Display for ExpressionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnexpectedCharacter(value) => write!(formatter, "unexpected character '{value}'"),
            Self::UnexpectedEnd => write!(formatter, "expression ended unexpectedly"),
            Self::UnknownVariable(name) => write!(formatter, "unknown variable '{name}'"),
            Self::DivisionByZero => write!(formatter, "division by zero"),
            Self::UnbalancedParenthesis => write!(formatter, "unbalanced parenthesis"),
            Self::NotFinite => write!(formatter, "result is not finite"),
            Self::UnknownFunction(name) => write!(formatter, "unknown function '{name}'"),
        }
    }
}

/// 求值。`variables` 提供当前实例可用的参数值。
pub fn evaluate(
    expression: &str,
    variables: &BTreeMap<String, f32>,
) -> Result<f32, ExpressionError> {
    let mut parser = Parser {
        chars: expression.chars().collect(),
        position: 0,
        variables,
    };
    let value = parser.parse_expression()?;
    parser.skip_whitespace();
    if parser.position < parser.chars.len() {
        return Err(ExpressionError::UnexpectedCharacter(
            parser.chars[parser.position],
        ));
    }
    if !value.is_finite() {
        return Err(ExpressionError::NotFinite);
    }
    Ok(value)
}

/// 校验表达式能否解析，不求值。用于写入时的快速反馈。
pub fn validate_syntax(expression: &str) -> Result<(), ExpressionError> {
    let empty = BTreeMap::new();
    // 用空变量集试解析：未定义变量不算语法问题，单独放行。
    match evaluate(expression, &empty) {
        Ok(_) | Err(ExpressionError::UnknownVariable(_)) => Ok(()),
        Err(error) => Err(error),
    }
}

struct Parser<'a> {
    chars: Vec<char>,
    position: usize,
    variables: &'a BTreeMap<String, f32>,
}

impl Parser<'_> {
    fn parse_expression(&mut self) -> Result<f32, ExpressionError> {
        let mut value = self.parse_term()?;
        loop {
            self.skip_whitespace();
            match self.peek() {
                Some('+') => {
                    self.position += 1;
                    value += self.parse_term()?;
                }
                Some('-') => {
                    self.position += 1;
                    value -= self.parse_term()?;
                }
                _ => return Ok(value),
            }
        }
    }

    fn parse_term(&mut self) -> Result<f32, ExpressionError> {
        let mut value = self.parse_factor()?;
        loop {
            self.skip_whitespace();
            match self.peek() {
                Some('*') => {
                    self.position += 1;
                    value *= self.parse_factor()?;
                }
                Some('/') => {
                    self.position += 1;
                    let divisor = self.parse_factor()?;
                    if divisor == 0.0 {
                        return Err(ExpressionError::DivisionByZero);
                    }
                    value /= divisor;
                }
                // 取模：环形布置里常用来判断「第几组」，例如 i % 4。
                Some('%') => {
                    self.position += 1;
                    let divisor = self.parse_factor()?;
                    if divisor == 0.0 {
                        return Err(ExpressionError::DivisionByZero);
                    }
                    value %= divisor;
                }
                _ => return Ok(value),
            }
        }
    }

    fn parse_factor(&mut self) -> Result<f32, ExpressionError> {
        self.skip_whitespace();
        if self.peek() == Some('-') {
            self.position += 1;
            return Ok(-self.parse_factor()?);
        }
        if self.peek() == Some('+') {
            self.position += 1;
            return self.parse_factor();
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Result<f32, ExpressionError> {
        self.skip_whitespace();
        match self.peek() {
            Some('(') => {
                self.position += 1;
                let value = self.parse_expression()?;
                self.skip_whitespace();
                if self.peek() != Some(')') {
                    return Err(ExpressionError::UnbalancedParenthesis);
                }
                self.position += 1;
                Ok(value)
            }
            Some(character) if character.is_ascii_digit() || character == '.' => {
                self.parse_number()
            }
            Some(character) if character.is_alphabetic() || character == '_' => {
                self.parse_variable()
            }
            Some(character) => Err(ExpressionError::UnexpectedCharacter(character)),
            None => Err(ExpressionError::UnexpectedEnd),
        }
    }

    fn parse_number(&mut self) -> Result<f32, ExpressionError> {
        let start = self.position;
        while matches!(self.peek(), Some(character) if character.is_ascii_digit() || character == '.')
        {
            self.position += 1;
        }
        let text: String = self.chars[start..self.position].iter().collect();
        text.parse::<f32>()
            .map_err(|_| ExpressionError::UnexpectedCharacter('?'))
    }

    fn parse_variable(&mut self) -> Result<f32, ExpressionError> {
        let start = self.position;
        while matches!(self.peek(), Some(character) if character.is_alphanumeric() || character == '_')
        {
            self.position += 1;
        }
        let name: String = self.chars[start..self.position].iter().collect();
        self.skip_whitespace();
        if self.peek() == Some('(') {
            return self.parse_call(&name);
        }
        // 常量优先，其次才是实例参数。
        match name.as_str() {
            "pi" => return Ok(std::f32::consts::PI),
            "tau" => return Ok(std::f32::consts::TAU),
            _ => {}
        }
        self.variables
            .get(&name)
            .copied()
            .ok_or(ExpressionError::UnknownVariable(name))
    }

    /// 单参数与双参数函数，覆盖构件布局所需的数学。
    fn parse_call(&mut self, name: &str) -> Result<f32, ExpressionError> {
        self.position += 1; // 跳过 '('
        let first = self.parse_expression()?;
        self.skip_whitespace();
        let second = if self.peek() == Some(',') {
            self.position += 1;
            let value = self.parse_expression()?;
            self.skip_whitespace();
            Some(value)
        } else {
            None
        };
        if self.peek() != Some(')') {
            return Err(ExpressionError::UnbalancedParenthesis);
        }
        self.position += 1;
        let result = match (name, second) {
            ("sin", None) => first.sin(),
            ("cos", None) => first.cos(),
            ("tan", None) => first.tan(),
            ("sqrt", None) => first.sqrt(),
            ("abs", None) => first.abs(),
            ("floor", None) => first.floor(),
            ("ceil", None) => first.ceil(),
            ("round", None) => first.round(),
            ("min", Some(other)) => first.min(other),
            ("max", Some(other)) => first.max(other),
            ("pow", Some(other)) => first.powf(other),
            _ => return Err(ExpressionError::UnknownFunction(name.to_string())),
        };
        if !result.is_finite() {
            return Err(ExpressionError::NotFinite);
        }
        Ok(result)
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.position).copied()
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(character) if character.is_whitespace()) {
            self.position += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, f32)]) -> BTreeMap<String, f32> {
        pairs
            .iter()
            .map(|(name, value)| ((*name).to_string(), *value))
            .collect()
    }

    #[test]
    fn evaluates_arithmetic_with_precedence() {
        assert_eq!(evaluate("1 + 2 * 3", &BTreeMap::new()).unwrap(), 7.0);
        assert_eq!(evaluate("(1 + 2) * 3", &BTreeMap::new()).unwrap(), 9.0);
        assert_eq!(evaluate("10 / 4", &BTreeMap::new()).unwrap(), 2.5);
        assert_eq!(evaluate("-3 + 5", &BTreeMap::new()).unwrap(), 2.0);
    }

    #[test]
    fn resolves_variables() {
        let context = vars(&[("radius", 13.0), ("wall", 0.45)]);
        assert!((evaluate("radius - wall / 2", &context).unwrap() - 12.775).abs() < 1e-5);
        assert!((evaluate("radius - wall - 0.55", &context).unwrap() - 12.0).abs() < 1e-5);
        assert!((evaluate("(radius * 2) / 4", &context).unwrap() - 6.5).abs() < 1e-5);
    }

    #[test]
    fn reports_unknown_variable() {
        let error = evaluate("radius * 2", &BTreeMap::new()).unwrap_err();
        assert_eq!(error, ExpressionError::UnknownVariable("radius".into()));
    }

    #[test]
    fn reports_division_by_zero() {
        assert_eq!(
            evaluate("1 / 0", &BTreeMap::new()).unwrap_err(),
            ExpressionError::DivisionByZero
        );
    }

    #[test]
    fn evaluates_functions_and_constants() {
        let context = vars(&[("i", 0.0), ("count", 48.0)]);
        assert!((evaluate("cos(i * tau / count)", &context).unwrap() - 1.0).abs() < 1e-5);
        assert!((evaluate("sin(pi / 2)", &context).unwrap() - 1.0).abs() < 1e-5);
        assert_eq!(evaluate("max(3, 7)", &BTreeMap::new()).unwrap(), 7.0);
        assert_eq!(evaluate("sqrt(16)", &BTreeMap::new()).unwrap(), 4.0);
        assert_eq!(evaluate("abs(-2.5)", &BTreeMap::new()).unwrap(), 2.5);
    }

    #[test]
    fn rejects_unknown_function() {
        assert!(matches!(
            evaluate("nope(1)", &BTreeMap::new()).unwrap_err(),
            ExpressionError::UnknownFunction(_)
        ));
    }

    #[test]
    fn supports_modulo() {
        let context = vars(&[("i", 7.0)]);
        assert_eq!(evaluate("i % 4", &context).unwrap(), 3.0);
        assert_eq!(evaluate("(i % 3) * 10", &context).unwrap(), 10.0);
    }

    #[test]
    fn validate_accepts_unknown_variables() {
        assert!(validate_syntax("radius - wall / 2").is_ok());
        assert!(validate_syntax("1 + * 2").is_err());
        assert!(validate_syntax("(1 + 2").is_err());
    }
}
