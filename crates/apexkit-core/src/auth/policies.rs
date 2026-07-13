use std::future::Future;
use std::pin::Pin;

use crate::Db;
use crate::auth::Claims;
use crate::query::ApexQuery;
use crate::query::filter::FilterNode;
use serde_json::{Value, json};
use std::iter::Peekable;
use std::str::Chars;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Read,
    Create,
    Update,
    Delete,
}

pub async fn check_access(
    policy_string: &str,
    user: Option<&Claims>,
    record_data: Option<&Value>,
    request_data: Option<&Value>,
    db: Option<Arc<dyn Db>>,
) -> bool {
    let policy = policy_string.trim();
    if policy.is_empty() {
        return false;
    }
    if policy == "public" {
        return true;
    }

    // Admins bypass RLS
    if user.is_some_and(|u| u.role == "admin") {
        return true;
    }

    if policy == "admin" {
        return false;
    }

    // --- NEW JSON POLICY EVALUATION ---
    if policy.starts_with('{') || policy.starts_with('[') {
        let mut json_val: Value = match serde_json::from_str(policy) {
            Ok(v) => v,
            Err(_) => return false,
        };

        if preprocess_policy(&mut json_val, user, request_data, db.as_ref())
            .await
            .is_err()
        {
            return false;
        }

        let node = FilterNode::parse(&json_val);
        return node.matches(record_data.unwrap_or(&json!({})));
    }

    // --- LEGACY STRING POLICY EVALUATION ---
    let tokens = match Tokenizer::new(policy).tokenize() {
        Ok(t) => t,
        Err(_) => {
            if policy.starts_with("owner:") {
                return check_owner_policy(policy, user, record_data);
            }
            return false;
        }
    };

    let mut parser = Parser::new(tokens);
    let ast = match parser.parse() {
        Ok(a) => a,
        Err(_) => {
            if policy.starts_with("owner:") {
                return check_owner_policy(policy, user, record_data);
            }
            return false;
        }
    };

    evaluate(&ast, user, record_data)
}

// Recursively processes the JSON Policy to resolve `@` prefixed variables dynamically.
// Uses std::pin::Pin and std::future::Future for zero-dependency async recursion.
pub fn preprocess_policy<'a>(
    val: &'a mut Value,
    user: Option<&'a Claims>,
    request_data: Option<&'a Value>,
    db: Option<&'a Arc<dyn Db>>,
) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>> {
    Box::pin(async move {
        let mut is_get = false;
        let mut get_payload = None;

        match val {
            Value::Object(map) => {
                // 1. Recurse into children first.
                // This ensures any variables (like @request.auth.id) used INSIDE
                // the @get() query payload are resolved before we run the query!
                for (_, v) in map.iter_mut() {
                    preprocess_policy(v, user, request_data, db).await?;
                }

                // 2. Check if this specific object is an @get() command
                if let Some(payload) = map.remove("@get()") {
                    is_get = true;
                    get_payload = Some(payload);
                }
            }
            Value::Array(arr) => {
                for v in arr.iter_mut() {
                    preprocess_policy(v, user, request_data, db).await?;
                }
            }
            Value::String(s) => {
                // If we match a string literal starting with @, resolve it dynamically
                if let Some(new_val) = resolve_literal(s, user, request_data).await? {
                    *val = new_val;
                }
            }
            _ => {}
        }

        // 3. Execute the @get() query and replace the current JSON node with the result array
        if is_get {
            if let Some(payload) = get_payload {
                let apex_query: ApexQuery = serde_json::from_value(payload)
                    .map_err(|e| format!("Invalid @get() query payload: {}", e))?;

                if let Some(d) = db {
                    // Execute the query using the shared Query Engine on the database
                    let res = d
                        .query_engine(apex_query)
                        .await
                        .map_err(|e| e.to_string())?;

                    if let Value::Array(arr) = res {
                        let mut flat = Vec::new();
                        for item in arr {
                            if let Value::Object(item_map) = item {
                                // Extract first key-value mapping to flatten array results
                                // e.g., [{"workspace_id": 1}, {"workspace_id": 2}] -> [1, 2]
                                if let Some((_, v)) = item_map.into_iter().next() {
                                    flat.push(v);
                                }
                            } else {
                                flat.push(item);
                            }
                        }
                        // Overwrite the entire {"@get()": {...}} object with the final Array
                        *val = Value::Array(flat);
                    }
                } else {
                    return Err(
                        "@get() is not supported in this context (missing db reference)".into(),
                    );
                }
            }
        }

        Ok(())
    })
}

// Evaluates and translates individual context string variables
async fn resolve_literal(
    s: &str,
    user: Option<&Claims>,
    request_data: Option<&Value>,
) -> Result<Option<Value>, String> {
    // 1. Resolve Auth Contexts
    if s == "@request.auth.id" {
        return Ok(Some(user.map(|u| json!(u.uid)).unwrap_or(Value::Null)));
    }
    if s == "@request.auth.role" {
        return Ok(Some(
            user.map(|u| json!(u.role.clone())).unwrap_or(Value::Null),
        ));
    }
    if s == "@request.auth.email" {
        return Ok(Some(
            user.map(|u| json!(u.sub.clone())).unwrap_or(Value::Null),
        ));
    }
    if s == "@request.auth" {
        return Ok(Some(
            user.map(|u| json!({ "id": u.uid, "role": u.role, "email": u.sub }))
                .unwrap_or(Value::Null),
        ));
    }

    // 2. Resolve Incoming Request Payload Contexts
    if s.starts_with("@request.record.") {
        let path = s.strip_prefix("@request.record.").unwrap();
        // Allow `@request.record.data.field` or `@request.record.field`
        let clean_path = path.strip_prefix("data.").unwrap_or(path);

        if let Some(req_data) = request_data {
            let mut current = req_data;
            for key in clean_path.split('.') {
                if let Some(v) = current.get(key) {
                    current = v;
                } else {
                    return Ok(Some(Value::Null));
                }
            }
            return Ok(Some(current.clone()));
        } else {
            return Ok(Some(Value::Null));
        }
    }

    Ok(None)
}

fn check_owner_policy(rule: &str, user: Option<&Claims>, record_data: Option<&Value>) -> bool {
    if let Some(u) = user {
        let field_name = &rule[6..];

        if let Some(data) = record_data
            && let Some(owner_val) = data.get(field_name)
        {
            if let Some(owner_id) = owner_val.as_i64() {
                return owner_id == u.uid;
            }
            if let Some(owner_str) = owner_val.as_str() {
                return owner_str == u.uid.to_string();
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
        Self {
            chars: input.chars().peekable(),
        }
    }

    fn tokenize(&mut self) -> Result<Vec<Token>, String> {
        let mut tokens = Vec::new();
        while let Some(&c) = self.chars.peek() {
            match c {
                ' ' | '\t' | '\n' | '\r' => {
                    self.chars.next();
                }
                '(' => {
                    tokens.push(Token::LParen);
                    self.chars.next();
                }
                ')' => {
                    tokens.push(Token::RParen);
                    self.chars.next();
                }
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
                        return Err("Expected ==".into());
                    }
                }
                '!' => {
                    self.chars.next();
                    if let Some('=') = self.chars.peek() {
                        self.chars.next();
                        tokens.push(Token::Neq);
                    } else {
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
                            if nc.is_alphanumeric()
                                || nc == '.'
                                || nc == ':'
                                || nc == '_'
                                || nc == '@'
                            {
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
    Binary {
        op: Token,
        left: Box<Expr>,
        right: Box<Expr>,
    },
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
            expr = Expr::Binary {
                op: Token::Or,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    fn parse_and(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_equality()?;
        while let Some(Token::And) = self.peek() {
            self.advance();
            let right = self.parse_equality()?;
            expr = Expr::Binary {
                op: Token::And,
                left: Box::new(expr),
                right: Box::new(right),
            };
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
                    expr = Expr::Binary {
                        op,
                        left: Box::new(expr),
                        right: Box::new(right),
                    };
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
        Expr::Binary { op, left, right } => match op {
            Token::And => evaluate(left, user, record) && evaluate(right, user, record),
            Token::Or => evaluate(left, user, record) || evaluate(right, user, record),
            Token::Eq => get_value(left, user, record) == get_value(right, user, record),
            Token::Neq => get_value(left, user, record) != get_value(right, user, record),
            _ => false,
        },
        Expr::Identifier(s) => match s.as_str() {
            "public" => true,
            "auth" => user.is_some(),
            "admin" => user.map(|u| u.role == "admin").unwrap_or(false),
            _ => {
                if s.starts_with("owner:") {
                    return check_owner_policy(s, user, record);
                }
                if let Some(val_str) = resolve_value(s, user, record) {
                    return val_str == "true" || val_str == "1";
                }
                false
            }
        },
        Expr::Literal(s) => !s.is_empty(),
    }
}

fn get_value(expr: &Expr, user: Option<&Claims>, record: Option<&Value>) -> String {
    match expr {
        Expr::Literal(s) => s.clone(),
        Expr::Identifier(s) => resolve_value(s, user, record).unwrap_or_default(),
        _ => "".to_string(),
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
    if let Some(stripped) = key.strip_prefix("field:") {
        return extract_field(stripped, record);
    }
    if key != "auth" && key != "admin" && key != "public" {
        return extract_field(key, record);
    }
    None
}

fn extract_field(field_name: &str, record: Option<&Value>) -> Option<String> {
    if let Some(rec) = record
        && let Some(val) = rec.get(field_name)
    {
        return match val {
            Value::String(v) => Some(v.clone()),
            Value::Number(n) => Some(n.to_string()),
            Value::Bool(b) => Some(b.to_string()),
            _ => Some(val.to_string()),
        };
    }
    None
}

// --- COMPILER FOR SQL PUSHDOWN ---

impl Expr {
    pub fn to_sql(&self, user: Option<&Claims>) -> String {
        match self {
            Expr::Binary { op, left, right } => {
                let l = left.to_sql(user);
                let r = right.to_sql(user);
                let sql_op = match op {
                    Token::And => "AND",
                    Token::Or => "OR",
                    Token::Eq => "=",
                    Token::Neq => "!=",
                    _ => "=",
                };
                format!("({} {} {})", l, sql_op, r)
            }
            Expr::Identifier(s) => match s.as_str() {
                "public" => "1=1".to_string(),
                "auth" => {
                    if user.is_some() {
                        "1=1".to_string()
                    } else {
                        "1=0".to_string()
                    }
                }
                "admin" => {
                    if user.is_some_and(|u| u.role == "admin") {
                        "1=1".to_string()
                    } else {
                        "1=0".to_string()
                    }
                }
                "auth.id" => user
                    .map(|u| format!("'{}'", u.uid))
                    .unwrap_or("NULL".to_string()),
                "auth.role" => user
                    .map(|u| format!("'{}'", u.role.replace("'", "''")))
                    .unwrap_or("NULL".to_string()),
                "auth.email" => user
                    .map(|u| format!("'{}'", u.sub.replace("'", "''")))
                    .unwrap_or("NULL".to_string()),
                _ => {
                    if let Some(field) = s.strip_prefix("field:") {
                        format!(
                            "json_extract(records.data, '$.{}')",
                            field.replace("'", "''")
                        )
                    } else if s == "true" || s == "false" {
                        s.to_string()
                    } else {
                        format!("'{}'", s.replace("'", "''"))
                    }
                }
            },
            Expr::Literal(s) => format!("'{}'", s.replace("'", "''")),
        }
    }
}

pub async fn compile_to_sql(
    policy_string: &str,
    user: Option<&Claims>,
    request_data: Option<&Value>,
    db: Option<Arc<dyn Db>>,
) -> Result<String, String> {
    let policy = policy_string.trim();
    if policy == "public" || policy.is_empty() {
        return Ok("1=1".to_string());
    }

    if user.is_some_and(|u| u.role == "admin") {
        return Ok("1=1".to_string());
    }

    if policy == "admin" {
        return Ok("1=0".to_string());
    }
    if policy == "auth" {
        return if user.is_some() {
            Ok("1=1".to_string())
        } else {
            Ok("1=0".to_string())
        };
    }

    // --- NEW JSON POLICY SQL COMPILATION ---
    if policy.starts_with('{') || policy.starts_with('[') {
        let mut json_val: Value = match serde_json::from_str(policy) {
            Ok(v) => v,
            Err(e) => return Err(e.to_string()),
        };

        preprocess_policy(&mut json_val, user, request_data, db.as_ref()).await?;

        let node = FilterNode::parse(&json_val);
        if let Some(inline_sql) = node.to_inline_sql() {
            return Ok(inline_sql);
        } else {
            return Ok("1=1".to_string()); // Empty filter
        }
    }

    if let Some(field_name) = policy.strip_prefix("owner:") {
        if let Some(u) = user {
            return Ok(format!(
                "json_extract(records.data, '$.{}') = '{}'",
                field_name.replace("'", "''"),
                u.uid
            ));
        } else {
            return Ok("1=0".to_string());
        }
    }

    let mut tokenizer = Tokenizer::new(policy);
    let tokens = tokenizer.tokenize()?;
    let mut parser = Parser::new(tokens);
    let ast = parser.parse()?;
    Ok(ast.to_sql(user))
}
