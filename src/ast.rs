/// Full Molang program consisting of one or more statements.
#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub statements: Vec<Statement>,
}

/// Executable unit of Molang. Complex expressions reduce to statement lists
/// so the interpreter can model control flow.
#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    /// Expression-only statement (value usually discarded unless it contains a return).
    Expr(Expr),
    /// Path assignment (temp./variable./context.).
    Assignment { target: Vec<String>, value: Expr },
    /// Nested block with its own statements.
    Block(Vec<Statement>),
    /// `loop(count, expr_or_block)`
    Loop { count: Expr, body: Box<Statement> },
    /// `for_each(variable, collection, expr_or_block)`
    ForEach {
        variable: Vec<String>,
        collection: Expr,
        body: Box<Statement>,
    },
    /// `return <expr?>`
    Return(Option<Expr>),
}

/// Expression tree used by both JIT and interpreter.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Number(f64),
    Path(Vec<String>),
    String(String),
    Array(Vec<Expr>),
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Conditional {
        condition: Box<Expr>,
        then_branch: Box<Expr>,
        else_branch: Option<Box<Expr>>,
    },
    Call {
        target: Box<Expr>,
        args: Vec<Expr>,
    },
    Flow(ControlFlowExpr),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Equal,
    NotEqual,
    And,
    Or,
    NullCoalesce,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Plus,
    Minus,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlFlowExpr {
    Break,
    Continue,
}

impl Program {
    /// Returns the single expression suitable for JIT compilation if present.
    pub fn as_jit_expression(&self) -> Option<&Expr> {
        if self.statements.len() == 1 {
            if let Statement::Expr(expr) = &self.statements[0] {
                if !expr.contains_flow() && expr.is_jit_compatible() {
                    return Some(expr);
                }
            }
        }
        None
    }
}

impl Expr {
    /// Returns true when the expression tree contains control-flow markers that the
    /// interpreter must honor (e.g., `break`, `continue`).
    pub fn contains_flow(&self) -> bool {
        match self {
            Expr::Number(_) | Expr::Path(_) | Expr::String(_) | Expr::Array(_) => false,
            Expr::Unary { expr, .. } => expr.contains_flow(),
            Expr::Binary { left, right, .. } => left.contains_flow() || right.contains_flow(),
            Expr::Conditional {
                condition,
                then_branch,
                else_branch,
            } => {
                condition.contains_flow()
                    || then_branch.contains_flow()
                    || else_branch
                        .as_ref()
                        .map(|expr| expr.contains_flow())
                        .unwrap_or(false)
            }
            Expr::Call { target, args } => {
                target.contains_flow() || args.iter().any(|expr| expr.contains_flow())
            }
            Expr::Flow(_) => true,
        }
    }

    /// Determines if the expression can be lowered to IR/JIT without interpreter support.
    pub fn is_jit_compatible(&self) -> bool {
        match self {
            Expr::Number(_) | Expr::Path(_) => true,
            Expr::Unary { expr, .. } => expr.is_jit_compatible(),
            Expr::Binary { left, right, .. } => {
                left.is_jit_compatible() && right.is_jit_compatible()
            }
            Expr::Conditional {
                condition,
                then_branch,
                else_branch,
            } => {
                condition.is_jit_compatible()
                    && then_branch.is_jit_compatible()
                    && else_branch
                        .as_ref()
                        .map(|expr| expr.is_jit_compatible())
                        .unwrap_or(true)
            }
            Expr::Call { target, args } => {
                target.is_jit_compatible() && args.iter().all(|expr| expr.is_jit_compatible())
            }
            Expr::String(_) | Expr::Array(_) | Expr::Flow(_) => false,
        }
    }
}
