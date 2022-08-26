use ast::types::*;
use ast::*;
use std::rc::Rc;

pub fn generate_ub() -> Statement {
    let ub_arr_name = "_wgslsmith_ub.arr";
    let ub_arr_index = "_wgslsmith_ub_index";
    let ub_arr_index_expr_node = ExprNode {
        data_type: DataType::Scalar(ScalarType::U32),
        expr: Expr::Var(VarExpr::new("_wgslsmith_ub_index")),
    };
    let mut block: Vec<Statement> = vec![
        // VarDeclStatement::new(
        //     ub_arr_name.to_string(),
        //     Some(DataType::Array(Rc::new(ScalarType::U32.into()), Some(1))),
        //     None,
        // ).into()
    ];
    block.push(Statement::ForLoop(ForLoopStatement::new(
        ForLoopHeader {
            init: Some(ForLoopInit::VarDecl(VarDeclStatement::new(
                ub_arr_index,
                Some(DataType::Scalar(ScalarType::U32)),
                Some(ExprNode {
                    data_type: DataType::Scalar(ScalarType::U32),
                    expr: Expr::Var(VarExpr::new("_wgslsmith_ub.min_index")),
                }),
            ))),
            condition: Some(ExprNode::from(BinOpExpr::new(
                BinOp::LessEqual,
                ub_arr_index_expr_node.clone(),
                ExprNode {
                    data_type: DataType::Scalar(ScalarType::U32),
                    expr: Expr::Var(VarExpr::new("_wgslsmith_ub.max_index")),
                },
            ))),
            update: Some(ForLoopUpdate::Assignment(AssignmentStatement {
                lhs: AssignmentLhs::Expr(LhsExprNode::name(
                    format!("({})", ub_arr_index).into(),
                    DataType::Scalar(ScalarType::U32),
                )),
                op: AssignmentOp::Plus,
                rhs: ExprNode::from(Lit::U32(1)),
            })),
        },
        vec![Statement::Assignment(AssignmentStatement::new(
            AssignmentLhs::name(
                format!("({})[{}]", ub_arr_name, ub_arr_index),
                DataType::Scalar(ScalarType::U32),
            ),
            AssignmentOp::Simple,
            ExprNode {
                data_type: DataType::Scalar(ScalarType::U32),
                expr: Expr::Var(VarExpr::new("_wgslsmith_ub.write_value")),
            },
        ))],
    )));
    Statement::Compound(block)
}
