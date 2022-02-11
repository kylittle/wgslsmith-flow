use std::collections::HashSet;
use std::fmt::Write;
use std::io::Read;
use std::rc::Rc;

use ast::types::{DataType, ScalarType};
use ast::{
    AssignmentLhs, AttrList, BinOp, Expr, ExprNode, FnDecl, FnInput, FnOutput, Lit, Postfix,
    Statement,
};

fn main() -> eyre::Result<()> {
    let input = read_stdin()?;
    let mut ast = parser::parse(&input);
    let mut reconditioner = Reconditioner::default();

    ast.entrypoint = reconditioner.recondition_fn(ast.entrypoint);

    let functions = ast
        .functions
        .into_iter()
        .map(|f| reconditioner.recondition_fn(f))
        .collect::<Vec<_>>();

    let wrappers = vector_safe_wrappers()
        .into_iter()
        .filter(|it| reconditioner.emit_fns.contains(&it.name));

    ast.functions = wrappers.chain(functions).collect();

    println!("{}", include_str!("prelude.wgsl"));

    if reconditioner.loop_var > 0 {
        println!(
            "var<private> LOOP_COUNTERS: array<u32, {}>;",
            reconditioner.loop_var
        );
        println!();
    }

    println!("{}", ast);

    Ok(())
}

fn read_stdin() -> eyre::Result<String> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    Ok(input)
}

#[derive(Default)]
struct Reconditioner {
    loop_var: u32,
    emit_fns: HashSet<String>,
}

impl Reconditioner {
    fn recondition_fn(&mut self, mut decl: FnDecl) -> FnDecl {
        decl.body = decl
            .body
            .into_iter()
            .map(|s| self.recondition_stmt(s))
            .collect();
        decl
    }

    fn recondition_stmt(&mut self, stmt: Statement) -> Statement {
        match stmt {
            Statement::LetDecl(ident, e) => Statement::LetDecl(ident, self.recondition_expr(e)),
            Statement::VarDecl(ident, e) => Statement::VarDecl(ident, self.recondition_expr(e)),
            Statement::Assignment(lhs, rhs) => {
                Statement::Assignment(lhs, self.recondition_expr(rhs))
            }
            Statement::Compound(s) => {
                Statement::Compound(s.into_iter().map(|s| self.recondition_stmt(s)).collect())
            }
            Statement::If(e, b) => Statement::If(
                self.recondition_expr(e),
                b.into_iter().map(|s| self.recondition_stmt(s)).collect(),
            ),
            Statement::Return(e) => Statement::Return(e.map(|e| self.recondition_expr(e))),
            Statement::Loop(s) => Statement::Loop({
                let id = self.loop_var();
                std::iter::once(Statement::If(
                    ExprNode {
                        data_type: DataType::Scalar(ScalarType::Bool),
                        expr: Expr::BinOp(
                            BinOp::GreaterEqual,
                            Box::new(ExprNode {
                                data_type: DataType::Scalar(ScalarType::U32),
                                expr: Expr::Postfix(
                                    Box::new(ExprNode {
                                        data_type: DataType::Array(Rc::new(DataType::Scalar(
                                            ScalarType::U32,
                                        ))),
                                        expr: Expr::Var("LOOP_COUNTERS".to_owned()),
                                    }),
                                    Postfix::ArrayIndex(Box::new(ExprNode {
                                        data_type: DataType::Scalar(ScalarType::U32),
                                        expr: Expr::Lit(Lit::UInt(id)),
                                    })),
                                ),
                            }),
                            Box::new(ExprNode {
                                data_type: DataType::Scalar(ScalarType::U32),
                                expr: Expr::Lit(Lit::UInt(1)),
                            }),
                        ),
                    },
                    vec![Statement::Break],
                ))
                .chain(std::iter::once(Statement::Assignment(
                    AssignmentLhs::Simple(
                        "LOOP_COUNTERS".to_owned(),
                        vec![Postfix::ArrayIndex(Box::new(ExprNode {
                            data_type: DataType::Scalar(ScalarType::U32),
                            expr: Expr::Lit(Lit::UInt(id)),
                        }))],
                    ),
                    ExprNode {
                        data_type: DataType::Scalar(ScalarType::U32),
                        expr: Expr::BinOp(
                            BinOp::Plus,
                            Box::new(ExprNode {
                                data_type: DataType::Scalar(ScalarType::U32),
                                expr: Expr::Postfix(
                                    Box::new(ExprNode {
                                        data_type: DataType::Array(Rc::new(DataType::Scalar(
                                            ScalarType::U32,
                                        ))),
                                        expr: Expr::Var("LOOP_COUNTERS".to_owned()),
                                    }),
                                    Postfix::ArrayIndex(Box::new(ExprNode {
                                        data_type: DataType::Scalar(ScalarType::U32),
                                        expr: Expr::Lit(Lit::UInt(id)),
                                    })),
                                ),
                            }),
                            Box::new(ExprNode {
                                data_type: DataType::Scalar(ScalarType::U32),
                                expr: Expr::Lit(Lit::UInt(1)),
                            }),
                        ),
                    },
                )))
                .chain(s.into_iter().map(|s| self.recondition_stmt(s)))
                .collect()
            }),
            Statement::Break => Statement::Break,
        }
    }

    fn recondition_expr(&mut self, expr: ExprNode) -> ExprNode {
        let reconditioned = match expr.expr {
            Expr::TypeCons(ty, args) => Expr::TypeCons(
                ty,
                args.into_iter().map(|e| self.recondition_expr(e)).collect(),
            ),
            Expr::UnOp(op, e) => Expr::UnOp(op, Box::new(self.recondition_expr(*e))),
            Expr::BinOp(op, l, r) => {
                let l = self.recondition_expr(*l);
                let r = self.recondition_expr(*r);
                return self.recondition_bin_op_expr(expr.data_type, op, l, r);
            }
            Expr::FnCall(name, args) => Expr::FnCall(
                name,
                args.into_iter().map(|e| self.recondition_expr(e)).collect(),
            ),
            e => e,
        };

        ExprNode {
            data_type: expr.data_type,
            expr: reconditioned,
        }
    }

    fn recondition_bin_op_expr(
        &mut self,
        data_type: DataType,
        op: BinOp,
        l: ExprNode,
        r: ExprNode,
    ) -> ExprNode {
        let name = match op {
            BinOp::Plus => self.safe_fn("PLUS", &data_type),
            BinOp::Minus => self.safe_fn("MINUS", &data_type),
            BinOp::Times => self.safe_fn("TIMES", &data_type),
            BinOp::Divide => self.safe_fn("DIVIDE", &data_type),
            BinOp::Mod => self.safe_fn("MOD", &data_type),
            op => {
                return ExprNode {
                    data_type,
                    expr: Expr::BinOp(op, Box::new(l), Box::new(r)),
                }
            }
        };

        ExprNode {
            data_type,
            expr: Expr::FnCall(name, vec![l, r]),
        }
    }

    fn loop_var(&mut self) -> u32 {
        let cur = self.loop_var;
        self.loop_var += 1;
        cur
    }

    fn safe_fn(&mut self, name: &str, data_type: &DataType) -> String {
        let ident = safe_fn(name, data_type);

        if !self.emit_fns.contains(&ident) {
            self.emit_fns.insert(ident.clone());
        }

        ident
    }
}

fn safe_fn(name: &str, data_type: &DataType) -> String {
    let mut ident = String::new();

    write!(ident, "SAFE_{}_", name).unwrap();

    match data_type {
        DataType::Scalar(ty) => write!(ident, "{}", ty).unwrap(),
        DataType::Vector(n, ty) => write!(ident, "vec{}_{}", n, ty).unwrap(),
        DataType::Array(_) => todo!(),
        DataType::User(_) => todo!(),
    }

    ident
}

/// Generates safe wrapper functions for vectors. These will forward to the correspoding safe scalar
/// wrapper for each vector component.
fn vector_safe_wrappers() -> Vec<FnDecl> {
    let mut fns = vec![];

    for op in ["PLUS", "MINUS", "TIMES", "DIVIDE", "MOD"] {
        for ty in [ScalarType::I32, ScalarType::U32] {
            for n in 2..=4 {
                let vec_ty = DataType::Vector(n, ty);
                fns.push(FnDecl {
                    attrs: AttrList(vec![]),
                    name: safe_fn(op, &vec_ty),
                    inputs: vec![
                        FnInput {
                            attrs: AttrList(vec![]),
                            name: "a".to_owned(),
                            data_type: vec_ty.clone(),
                        },
                        FnInput {
                            attrs: AttrList(vec![]),
                            name: "b".to_owned(),
                            data_type: vec_ty.clone(),
                        },
                    ],
                    output: Some(FnOutput {
                        attrs: AttrList(vec![]),
                        data_type: vec_ty.clone(),
                    }),
                    body: vec![Statement::Return(Some(ExprNode {
                        data_type: vec_ty.clone(),
                        expr: Expr::TypeCons(
                            vec_ty.clone(),
                            (0..n)
                                .map(|i| {
                                    let component = match i {
                                        0 => "x",
                                        1 => "y",
                                        2 => "z",
                                        3 => "w",
                                        _ => unreachable!(),
                                    };

                                    ExprNode {
                                        data_type: DataType::Scalar(ty),
                                        expr: Expr::FnCall(
                                            safe_fn(op, &DataType::Scalar(ty)),
                                            vec![
                                                ExprNode {
                                                    data_type: DataType::Scalar(ty),
                                                    expr: Expr::Postfix(
                                                        Box::new(ExprNode {
                                                            data_type: vec_ty.clone(),
                                                            expr: Expr::Var("a".to_owned()),
                                                        }),
                                                        Postfix::Member(component.to_owned()),
                                                    ),
                                                },
                                                ExprNode {
                                                    data_type: DataType::Scalar(ty),
                                                    expr: Expr::Postfix(
                                                        Box::new(ExprNode {
                                                            data_type: vec_ty.clone(),
                                                            expr: Expr::Var("b".to_owned()),
                                                        }),
                                                        Postfix::Member(component.to_owned()),
                                                    ),
                                                },
                                            ],
                                        ),
                                    }
                                })
                                .collect(),
                        ),
                    }))],
                });
            }
        }
    }

    fns
}
