use crate::nvim::error::{NvimError, Result};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum TypVal {
    Number(i64),
    String(String),
    Boolean(bool),
    Float(f64),
    List(Vec<TypVal>),
    Dictionary(HashMap<String, TypVal>),
    Nil,
}

impl TypVal {
    pub fn is_truthy(&self) -> bool {
        match self {
            TypVal::Number(n) => *n != 0,
            TypVal::Boolean(b) => *b,
            TypVal::String(s) => !s.is_empty(),
            TypVal::Float(f) => *f != 0.0,
            TypVal::List(l) => !l.is_empty(),
            TypVal::Dictionary(d) => !d.is_empty(),
            TypVal::Nil => false,
        }
    }
}

use crate::nvim::state::VimState;

pub type BuiltinFunc = fn(&VimState, Vec<TypVal>) -> Result<TypVal>;

pub struct EvalContext {
    pub variables: HashMap<String, TypVal>,
    pub builtins: HashMap<String, BuiltinFunc>,
}

impl EvalContext {
    pub fn new() -> Self {
        let mut ctx = Self {
            variables: HashMap::new(),
            builtins: HashMap::new(),
        };
        ctx.variables.insert("v:true".to_string(), TypVal::Boolean(true));
        ctx.variables.insert("v:false".to_string(), TypVal::Boolean(false));
        ctx.variables.insert("v:null".to_string(), TypVal::Nil);
        ctx.variables.insert("v:none".to_string(), TypVal::Nil);
        ctx.variables.insert("v:exiting".to_string(), TypVal::Nil);
        
        ctx.register_builtin("strlen", |_, args| {
            if let Some(TypVal::String(s)) = args.first() {
                Ok(TypVal::Number(s.chars().count() as i64))
            } else {
                Err(NvimError::Api("strlen() expects a string".to_string()))
            }
        });

        ctx.register_builtin("getline", |state, args| {
            let lnum = match args.first() {
                Some(TypVal::String(s)) => {
                    match s.as_str() {
                        "." => state.current_window().cursor().row,
                        "$" => state.current_window().buffer().borrow().line_count(),
                        _ => s.parse::<usize>().unwrap_or(0),
                    }
                }
                Some(TypVal::Number(n)) => *n as usize,
                _ => return Err(NvimError::Api("getline() expects a line number".to_string())),
            };
            let buf = state.current_window().buffer();
            let res = if let Ok(b) = buf.try_borrow() {
                b.get_line(lnum).unwrap_or("").to_string()
            } else {
                "".to_string()
            };
            Ok(TypVal::String(res))
        });

        ctx.register_builtin("setline", |state, args| {
            let lnum = match args.get(0) {
                Some(TypVal::String(s)) => {
                    match s.as_str() {
                        "." => state.current_window().cursor().row,
                        "$" => state.current_window().buffer().borrow().line_count(),
                        _ => s.parse::<usize>().unwrap_or(0),
                    }
                }
                Some(TypVal::Number(n)) => *n as usize,
                _ => return Err(NvimError::Api("setline() expects a line number".to_string())),
            };
            let text = match args.get(1) {
                Some(TypVal::String(s)) => s.clone(),
                _ => return Err(NvimError::Api("setline() expects text".to_string())),
            };
            let buf = state.current_window().buffer();
            if let Ok(mut b) = buf.try_borrow_mut() {
                let _ = b.set_line(lnum, 0, &text);
            }
            Ok(TypVal::Number(0))
        });
        
        ctx
    }

    pub fn register_builtin(&mut self, name: &str, func: BuiltinFunc) {
        self.builtins.insert(name.to_string(), func);
    }
    
    pub fn execute(&mut self, state: &VimState, script: &str) -> Result<()> {
        let lines: Vec<&str> = script.lines().collect();
        let mut i = 0;
        while i < lines.len() {
            let line = lines[i].trim();
            if line.is_empty() || line.starts_with('"') {
                i += 1;
                continue;
            }
            if line.starts_with("if") {
                let expr = &line[2..].trim();
                let condition = self.eval(state, expr)?;
                if !condition.is_truthy() {
                    let mut depth = 1;
                    while i + 1 < lines.len() && depth > 0 {
                        i += 1;
                        let next_line = lines[i].trim();
                        if next_line.starts_with("if") { depth += 1; }
                        else if next_line == "endif" { depth -= 1; }
                    }
                }
            } else if line.starts_with("let") {
                let parts: Vec<&str> = line[3..].splitn(2, '=').collect();
                if parts.len() == 2 {
                    let name = parts[0].trim();
                    let val = self.eval(state, parts[1].trim())?;
                    self.set_var(name, val);
                }
            }
            i += 1;
        }
        Ok(())
    }

    pub fn eval(&self, state: &VimState, expr: &str) -> Result<TypVal> {
        let expr = expr.trim();
        if expr.is_empty() { return Ok(TypVal::String("".to_string())); }
        
        if expr.ends_with(')') {
            if let Some(open_paren) = expr.find('(') {
                let func_name = expr[..open_paren].trim();
                let args_str = &expr[open_paren+1..expr.len()-1];
                let args = if args_str.trim().is_empty() {
                    Vec::new()
                } else {
                    let mut args = Vec::new();
                    let mut current = String::new();
                    let mut in_quotes = false;
                    let mut paren_depth = 0;
                    for c in args_str.chars() {
                        if c == '"' { in_quotes = !in_quotes; current.push(c); }
                        else if !in_quotes && c == '(' { paren_depth += 1; current.push(c); }
                        else if !in_quotes && c == ')' { paren_depth -= 1; current.push(c); }
                        else if c == ',' && !in_quotes && paren_depth == 0 {
                            args.push(self.eval(state, current.trim())?);
                            current.clear();
                        } else {
                            current.push(c);
                        }
                    }
                    if !current.trim().is_empty() || args_str.trim().ends_with("\"\"") {
                        args.push(self.eval(state, current.trim())?);
                    }
                    args
                };
                return self.call_builtin(state, func_name, args);
            }
        }

        let mut in_quotes = false;
        let mut op_idx = None;
        let mut op_char = '+';
        
        let chars_vec: Vec<char> = expr.chars().collect();
        for (i, &c) in chars_vec.iter().enumerate().rev() {
            if c == '"' { in_quotes = !in_quotes; }
            else if !in_quotes && (c == '+' || c == '-' || c == '*') {
                op_idx = Some(i);
                op_char = c;
                break;
            }
        }

        if let Some(idx) = op_idx {
            let left = self.eval(state, &expr[..idx])?;
            let right = self.eval(state, &expr[idx+1..])?;
            return match (left, right) {
                (TypVal::Number(a), TypVal::Number(b)) => {
                    match op_char {
                        '+' => Ok(TypVal::Number(a + b)),
                        '-' => Ok(TypVal::Number(a - b)),
                        '*' => Ok(TypVal::Number(a * b)),
                        _ => unreachable!(),
                    }
                }
                (TypVal::Float(a), TypVal::Float(b)) => {
                    match op_char {
                        '+' => Ok(TypVal::Float(a + b)),
                        '-' => Ok(TypVal::Float(a - b)),
                        '*' => Ok(TypVal::Float(a * b)),
                        _ => unreachable!(),
                    }
                }
                (TypVal::String(a), TypVal::String(b)) if op_char == '+' => Ok(TypVal::String(format!("{}{}", a, b))),
                _ => Err(NvimError::Api(format!("Type mismatch in '{}'", op_char))),
            };
        }

        let normalized = self.normalize_name(expr);
        if let Some(val) = self.variables.get(&normalized) { return Ok(val.clone()); }
        if let Some(val) = self.variables.get(expr) { return Ok(val.clone()); }
        if let Ok(n) = expr.parse::<i64>() { return Ok(TypVal::Number(n)); }
        if let Ok(f) = expr.parse::<f64>() { return Ok(TypVal::Float(f)); }
        if expr == "\"\"" {
            return Ok(TypVal::String("".to_string()));
        }
        if expr.len() >= 2 && expr.starts_with('"') && expr.ends_with('"') {
            return Ok(TypVal::String(expr[1..expr.len()-1].to_string()));
        }
        Err(NvimError::Api(format!("Unknown expression: {}", expr)))
    }

    fn call_builtin(&self, state: &VimState, name: &str, args: Vec<TypVal>) -> Result<TypVal> {
        if let Some(func) = self.builtins.get(name) {
            func(state, args)
        } else {
            Err(NvimError::Api(format!("Unknown function: {}", name)))
        }
    }

    pub fn set_var(&mut self, name: &str, val: TypVal) {
        let normalized = self.normalize_name(name);
        self.variables.insert(normalized, val);
    }

    fn normalize_name(&self, name: &str) -> String {
        if name.contains(':') { name.to_string() } else { format!("g:{}", name) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nvim::state::VimState;

    #[test]
    fn test_eval_basic_types() {
        let state = VimState::new();
        assert_eq!(state.eval_context.eval(&state, "123").unwrap(), TypVal::Number(123));
        assert_eq!(state.eval_context.eval(&state, "\"foo\"").unwrap(), TypVal::String("foo".to_string()));
        assert_eq!(state.eval_context.eval(&state, "v:true").unwrap(), TypVal::Boolean(true));
    }

    #[test]
    fn test_eval_binary_ops() {
        let state = VimState::new();
        assert_eq!(state.eval_context.eval(&state, "1 + 2").unwrap(), TypVal::Number(3));
        assert_eq!(state.eval_context.eval(&state, "\"a\" + \"b\"").unwrap(), TypVal::String("ab".to_string()));
        assert_eq!(state.eval_context.eval(&state, "1.5 + 2.5").unwrap(), TypVal::Float(4.0));
        assert_eq!(state.eval_context.eval(&state, "10 - 3").unwrap(), TypVal::Number(7));
        assert_eq!(state.eval_context.eval(&state, "2 * 5").unwrap(), TypVal::Number(10));
        assert_eq!(state.eval_context.eval(&state, "5.0 - 1.0").unwrap(), TypVal::Float(4.0));
        assert_eq!(state.eval_context.eval(&state, "3.0 * 2.0").unwrap(), TypVal::Float(6.0));
    }

    #[test]
    fn test_eval_builtins() {
        let state = VimState::new();
        assert_eq!(state.eval_context.eval(&state, "strlen(\"abc\")").unwrap(), TypVal::Number(3));
    }

    #[test]
    fn test_getline_setline() {
        let state = VimState::new();
        state.current_window().buffer().borrow_mut().append_line("hello").unwrap();
        assert_eq!(state.eval_context.eval(&state, "getline(1)").unwrap(), TypVal::String("hello".to_string()));
        let _ = state.eval_context.eval(&state, "setline(1, \"world\")");
        assert_eq!(state.current_window().buffer().borrow().get_line(1).unwrap(), "world");
    }

    #[test]
    fn test_eval_variables() {
        let state = VimState::new();
        let mut ec = EvalContext::new();
        ec.execute(&state, "let x = 10").unwrap();
        assert_eq!(ec.eval(&state, "x").unwrap(), TypVal::Number(10));
    }

    #[test]
    fn test_eval_control_flow() {
        let state = VimState::new();
        let mut ec = EvalContext::new();
        let script = "
            let a = 1
            if a
                let b = 2
            endif
            if 0
                let c = 3
            endif
        ";
        ec.execute(&state, script).unwrap();
        assert_eq!(ec.eval(&state, "b").unwrap(), TypVal::Number(2));
        assert!(ec.eval(&state, "c").is_err());
    }

    #[test]
    fn test_eval_truthy() {
        assert!(TypVal::Number(1).is_truthy());
        assert!(!TypVal::Number(0).is_truthy());
        assert!(TypVal::String("s".to_string()).is_truthy());
        assert!(!TypVal::String("".to_string()).is_truthy());
        assert!(TypVal::List(vec![TypVal::Number(1)]).is_truthy());
        assert!(!TypVal::List(vec![]).is_truthy());
    }
}
