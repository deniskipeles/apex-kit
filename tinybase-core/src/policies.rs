// =========================== /teamspace/studios/this_studio/tinybase/tinybase/tinybase-core/src/policies.rs ===========================
use crate::auth::Claims;
use serde_json::Value;
use std::iter::Peekable;
use std::str::Chars;

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Read,
    Create,
    Update,
    Delete,
}

pub fn check_access(
    policy_string: &str,
    user: Option<&Claims>,
    record_data: Option<&Value>,
) -> bool {
    let policy = policy_string.trim();
    if policy.is_empty() { return false; } // Default deny if empty? Or public? Usually public means empty string in some systems, but here "public" is explicit.
    
    if policy == "public" { return true; }
    
    // Fast path for simple admin check to avoid parsing
    if policy == "admin" { 
        return user.map(|u| u.role == "admin").unwrap_or(false); 
    }

    // Attempt to tokenize
    let tokens = match Tokenizer::new(policy).tokenize() {
        Ok(t) => t,
        Err(_) => {
            // Fallback for legacy owner: syntax if tokenization failed
            if policy.starts_with("owner:") {
                return check_owner_policy(policy, user, record_data);
            }
            return false;
        }
    };

    // Attempt to parse
    let mut parser = Parser::new(tokens);
    let ast = match parser.parse() {
        Ok(a) => a,
        Err(_) => {
             // Fallback
             if policy.starts_with("owner:") {
                return check_owner_policy(policy, user, record_data);
            }
            return false;
        }
    };

    // Evaluate
    evaluate(&ast, user, record_data)
}

fn check_owner_policy(rule: &str, user: Option<&Claims>, record_data: Option<&Value>) -> bool {
    if let Some(u) = user {
        if u.role == "admin" { return true; }
        
        let field_name = &rule[6..]; // strip "owner:"
        
        if let Some(data) = record_data {
            if let Some(owner_val) = data.get(field_name) {
                // Handle number/string conversions
                if let Some(owner_id) = owner_val.as_i64() {
                    return owner_id == u.uid;
                }
                if let Some(owner_str) = owner_val.as_str() {
                    return owner_str == u.uid.to_string();
                }
            }
        }
        false
    } else {
        false
    }
}

// --- EXPRESSION ENGINE ---

#[derive(Debug, Clone, PartialEq)]
enum Token {
    LParen,
    RParen,
    And,
    Or,
    Eq,
    Neq,
    String(String),
    Identifier(String), 
}

struct Tokenizer<'a> {
    chars: Peekable<Chars<'a>>,
}

impl<'a> Tokenizer<'a> {
    fn new(input: &'a str) -> Self {
        Self { chars: input.chars().peekable() }
    }

    fn tokenize(&mut self) -> Result<Vec<Token>, String> {
        let mut tokens = Vec::new();
        while let Some(&c) = self.chars.peek() {
            match c {
                ' ' | '\t' | '\n' | '\r' => { self.chars.next(); }
                '(' => { tokens.push(Token::LParen); self.chars.next(); }
                ')' => { tokens.push(Token::RParen); self.chars.next(); }
                '&' => {
                    self.chars.next();
                    if let Some('&') = self.chars.peek() {
                        self.chars.next();
                        tokens.push(Token::And);
                    } else {
                        return Err("Expected &&".into());
                    }
                }
                '|' => {
                    self.chars.next();
                    if let Some('|') = self.chars.peek() {
                        self.chars.next();
                        tokens.push(Token::Or);
                    } else {
                        return Err("Expected ||".into());
                    }
                }
                '=' => {
                    self.chars.next();
                    if let Some('=') = self.chars.peek() {
                        self.chars.next();
                        tokens.push(Token::Eq);
                    } else {
                        // Allow single = as equality for simplicity? 
                        // Standard is ==. Let's enforce == for robustness.
                        return Err("Expected ==".into());
                    }
                }
                '!' => {
                    self.chars.next();
                    if let Some('=') = self.chars.peek() {
                        self.chars.next();
                        tokens.push(Token::Neq);
                    } else {
                        // Could be Not (!), but not implementing unary for now based on req
                        return Err("Expected !=".into());
                    }
                }
                '"' | '\'' => {
                    let quote_char = c;
                    self.chars.next();
                    let mut s = String::new();
                    let mut escaped = false;
                    loop {
                        match self.chars.next() {
                            Some(nc) if nc == quote_char && !escaped => break,
                            Some('\\') if !escaped => escaped = true,
                            Some(nc) => {
                                s.push(nc);
                                escaped = false;
                            }
                            None => return Err("Unterminated string".into()),
                        }
                    }
                    tokens.push(Token::String(s));
                }
                _ => {
                    if c.is_alphanumeric() || c == '.' || c == ':' || c == '_' || c == '@' {
                        let mut s = String::new();
                        while let Some(&nc) = self.chars.peek() {
                            if nc.is_alphanumeric() || nc == '.' || nc == ':' || nc == '_' || nc == '@' {
                                s.push(nc);
                                self.chars.next();
                            } else {
                                break;
                            }
                        }
                        tokens.push(Token::Identifier(s));
                    } else {
                        return Err(format!("Unexpected char: {}", c));
                    }
                }
            }
        }
        Ok(tokens)
    }
}

#[derive(Debug)]
enum Expr {
    Binary { op: Token, left: Box<Expr>, right: Box<Expr> },
    Literal(String),
    Identifier(String),
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn parse(&mut self) -> Result<Expr, String> {
        if self.tokens.is_empty() {
            return Err("Empty expression".into());
        }
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_and()?;
        while let Some(Token::Or) = self.peek() {
            self.advance();
            let right = self.parse_and()?;
            expr = Expr::Binary { op: Token::Or, left: Box::new(expr), right: Box::new(right) };
        }
        Ok(expr)
    }

    fn parse_and(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_equality()?;
        while let Some(Token::And) = self.peek() {
            self.advance();
            let right = self.parse_equality()?;
            expr = Expr::Binary { op: Token::And, left: Box::new(expr), right: Box::new(right) };
        }
        Ok(expr)
    }

    fn parse_equality(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_primary()?;
        if let Some(op) = self.peek() {
            match op {
                Token::Eq | Token::Neq => {
                    let op = op.clone();
                    self.advance();
                    let right = self.parse_primary()?;
                    expr = Expr::Binary { op, left: Box::new(expr), right: Box::new(right) };
                }
                _ => {}
            }
        }
        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expr, String> {
        match self.peek() {
            Some(Token::Identifier(s)) => {
                let s = s.clone();
                self.advance();
                Ok(Expr::Identifier(s))
            }
            Some(Token::String(s)) => {
                let s = s.clone();
                self.advance();
                Ok(Expr::Literal(s))
            }
            Some(Token::LParen) => {
                self.advance();
                let expr = self.parse_or()?;
                if let Some(Token::RParen) = self.peek() {
                    self.advance();
                    Ok(expr)
                } else {
                    Err("Expected )".into())
                }
            }
            None => Err("Unexpected end of expression".into()),
            _ => Err("Unexpected token in primary expression".into()),
        }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn advance(&mut self) {
        self.pos += 1;
    }
}

fn evaluate(expr: &Expr, user: Option<&Claims>, record: Option<&Value>) -> bool {
    match expr {
        Expr::Binary { op, left, right } => {
            match op {
                Token::And => evaluate(left, user, record) && evaluate(right, user, record),
                Token::Or => evaluate(left, user, record) || evaluate(right, user, record),
                Token::Eq => {
                    let l_val = get_value(left, user, record);
                    let r_val = get_value(right, user, record);
                    l_val == r_val
                },
                Token::Neq => {
                    let l_val = get_value(left, user, record);
                    let r_val = get_value(right, user, record);
                    l_val != r_val
                },
                _ => false,
            }
        },
        Expr::Identifier(s) => {
            match s.as_str() {
                "public" => true,
                "auth" => user.is_some(),
                "admin" => user.map(|u| u.role == "admin").unwrap_or(false),
                _ => {
                    // Check legacy "owner:" prefix used as boolean condition
                    if s.starts_with("owner:") {
                        return check_owner_policy(s, user, record);
                    }
                    // Generic field truthiness check (e.g. "is_published")
                    if let Some(val_str) = resolve_value(s, user, record) {
                        return val_str == "true" || val_str == "1";
                    }
                    false
                }
            }
        },
        Expr::Literal(s) => !s.is_empty(), // Non-empty string literal is true
    }
}

// Helper to resolve string value for comparisons
fn get_value(expr: &Expr, user: Option<&Claims>, record: Option<&Value>) -> String {
    match expr {
        Expr::Literal(s) => s.clone(),
        Expr::Identifier(s) => resolve_value(s, user, record).unwrap_or_default(),
        _ => "".to_string(), // Binary expressions don't resolve to string values in this simple DSL
    }
}

fn resolve_value(key: &str, user: Option<&Claims>, record: Option<&Value>) -> Option<String> {
    if key == "auth.id" {
        return user.map(|u| u.uid.to_string());
    }
    if key == "auth.role" {
        return user.map(|u| u.role.clone());
    }
    if key == "auth.email" {
        return user.map(|u| u.sub.clone());
    }
    
    // Support "field:name" syntax
    if let Some(stripped) = key.strip_prefix("field:") {
        return extract_field(stripped, record);
    }
    
    // Support direct "name" syntax if it's not a keyword?
    // Let's stick to "field:" or try direct record access if not a keyword
    if key != "auth" && key != "admin" && key != "public" {
        if let Some(val) = extract_field(key, record) {
            return Some(val);
        }
    }
    
    None
}

fn extract_field(field_name: &str, record: Option<&Value>) -> Option<String> {
    if let Some(rec) = record {
        if let Some(val) = rec.get(field_name) {
            return match val {
                Value::String(v) => Some(v.clone()),
                Value::Number(n) => Some(n.to_string()),
                Value::Bool(b) => Some(b.to_string()),
                _ => Some(val.to_string())
            };
        }
    }
    None
}