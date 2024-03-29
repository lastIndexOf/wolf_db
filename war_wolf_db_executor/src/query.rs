use war_wolf_db_metadata::{
    func::func_exists,
    table::{columns_exists, table_exists},
    Metadata, FUNC, TABLE,
};
use war_wolf_db_sql::parser::ast::{self, Clause, Ident, Literal};

use crate::query::{
    operator::{CondVal, Condition, GroupBy},
    query_op::FuncColumn,
};

use self::{
    operator::{Filter, Join, Order, Scan},
    query_op::{Column, Query, TableColumn},
};

pub mod operator;
mod query_op;

#[derive(Debug, Default, PartialEq, Clone)]
pub struct QueryBuilder {
    project_columns: Vec<Column>,
    filter_operators: Vec<Filter>,
    scan_operators: Vec<Scan>,
    join_operator: Option<Join>,
    order_operator: Option<Order>,
    group_column: Option<GroupBy>,
}

impl QueryBuilder {
    pub fn new() -> Self {
        todo!()
    }

    pub fn build(self) -> Query {
        todo!()
    }

    pub fn with_stmt(self, stmt: &ast::Stmt) -> Self {
        // assert!(matches!(stmt, ast::Stmt::SelectStmt { .. }));

        let mut this = self;

        match stmt {
            ast::Stmt::SelectStmt {
                select,
                from,
                condition,
                ordering,
                group_by,
            } => {
                this = this.with_from_clause(from);
                this = this.with_select_clause(select);

                if let Some(clause) = condition {
                    this = this.with_where_clause(clause);
                }

                this = this.with_join_clause(from);

                if let Some(clause) = ordering {
                    this = this.with_order_clause(clause);
                }

                if let Some(clause) = group_by {
                    this = this.with_group_by_clause(clause);
                }
            }
            _ => todo!(),
        }

        this.optimize();

        this
    }

    #[inline]
    fn with_from_clause(mut self, clause: &Clause) -> Self {
        assert!(matches!(clause, Clause::FromClause(_, _)));

        let mut unchecked_tables = vec![];
        let mut checked_tables = vec![];

        match clause {
            Clause::FromClause(name, join_clause) => {
                unchecked_tables.push(name.to_string());

                if let Some(join_clause) = join_clause {
                    // handle join clause
                    if let Clause::JoinClause { join_on, .. } = join_clause.as_ref() {
                        unchecked_tables.push(join_on.to_string());
                    }
                }
            }
            _ => {}
        }

        for table_name in unchecked_tables {
            // check if table exists
            // if table exists, add to checked_tables
            // else, return error
            let rets = TABLE
                .get()
                .unwrap()
                .select(|table| table.name == table_name);

            if rets.is_empty() {
                // TODO: add custom error
                panic!("Table {} does not exist", table_name);
            }

            checked_tables.push(table_name);
        }

        for table_name in checked_tables {
            let table = TABLE
                .get()
                .unwrap()
                .select(|table| table.name == table_name)[0];

            let scan = Scan {
                table_name: table.name.clone(),
                columns: table.columns.clone(),
            };

            self.scan_operators.push(scan);
        }

        self
    }

    #[inline]
    fn with_select_clause(mut self, clause: &Clause) -> Self {
        assert!(matches!(clause, Clause::SelectClause(_)));

        let mut table_cols = vec![];

        match clause {
            Clause::SelectClause(exprs) => {
                for expr in exprs {
                    match expr {
                        ast::Expr::LiteralExpr(Literal::Star) => {
                            if !table_cols.is_empty() {
                                // TODO: add custom error
                                panic!("* cannot be used with other columns");
                            }

                            for scan in &self.scan_operators {
                                let table = TABLE
                                    .get()
                                    .unwrap()
                                    .select(|table| table.name == scan.table_name)[0];

                                for column in &table.columns {
                                    table_cols.push(Column::TableColumn(TableColumn {
                                        table_name: table.name.clone(),
                                        column: column.name.clone(),
                                    }));
                                }
                            }

                            break;
                        }
                        ast::Expr::DotExpr(tb, col) => match (tb.as_ref(), col.as_ref()) {
                            (ast::Expr::IdentExpr(tb), ast::Expr::IdentExpr(col)) => {
                                let table_name = tb.to_string();
                                let column_name = col.to_string();

                                if !columns_exists(&TABLE.get().unwrap(), &table_name, &column_name)
                                {
                                    // TODO: add custom error
                                    panic!(
                                        "Column {} does not exist in table {}",
                                        column_name, table_name
                                    );
                                }

                                table_cols.push(Column::TableColumn(TableColumn {
                                    table_name: table_name.to_string(),
                                    column: column_name.to_string(),
                                }));
                            }
                            _ => {
                                // TODO: add custom error
                                panic!("Invalid expression in select clause: {:?}", expr);
                            }
                        },
                        ast::Expr::IdentExpr(Ident(ident)) => {
                            let mut founded = false;

                            for scan in &self.scan_operators {
                                let table_md = TABLE.get().unwrap();

                                if columns_exists(table_md, &scan.table_name, &ident) {
                                    if founded {
                                        // TODO: add custom error
                                        panic!("Column {} is ambiguous", ident);
                                    }

                                    table_cols.push(Column::TableColumn(TableColumn {
                                        table_name: scan.table_name.clone(),
                                        column: ident.clone(),
                                    }));

                                    founded = true;
                                }
                            }

                            if !founded {
                                // TODO: add custom error
                                panic!(
                                    "Column {} does not exist in tables {}",
                                    ident,
                                    self.scan_operators
                                        .iter()
                                        .map(|scan| { &scan.table_name[..] })
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                );
                            }
                        }
                        ast::Expr::FnCallExpr { name, arguments } => {
                            let mut args = vec![];
                            for arg in arguments {
                                match arg {
                                    ast::Expr::IdentExpr(Ident(ident)) => {
                                        let mut founded = false;

                                        for scan in &self.scan_operators {
                                            let table_md = TABLE.get().unwrap();

                                            if columns_exists(table_md, &scan.table_name, &ident) {
                                                if founded {
                                                    // TODO: add custom error
                                                    panic!("Column {} is ambiguous", ident);
                                                }

                                                args.push(TableColumn {
                                                    table_name: scan.table_name.clone(),
                                                    column: ident.clone(),
                                                });

                                                founded = true;
                                            }
                                        }

                                        if !founded {
                                            // TODO: add custom error
                                            panic!(
                                                "Column {} does not exist in tables {}",
                                                ident,
                                                self.scan_operators
                                                    .iter()
                                                    .map(|scan| { &scan.table_name[..] })
                                                    .collect::<Vec<_>>()
                                                    .join(", ")
                                            );
                                        }
                                    }
                                    ast::Expr::DotExpr(tb, col) => {
                                        match (tb.as_ref(), col.as_ref()) {
                                            (
                                                ast::Expr::IdentExpr(tb),
                                                ast::Expr::IdentExpr(col),
                                            ) => {
                                                let table_name = tb.to_string();
                                                let column_name = col.to_string();

                                                if !columns_exists(
                                                    &TABLE.get().unwrap(),
                                                    &table_name,
                                                    &column_name,
                                                ) {
                                                    // TODO: add custom error
                                                    panic!(
                                                        "Column {} does not exist in table {}",
                                                        column_name, table_name
                                                    );
                                                }

                                                args.push(TableColumn {
                                                    table_name: table_name.to_string(),
                                                    column: column_name.to_string(),
                                                });
                                            }
                                            _ => {
                                                // TODO: add custom error
                                                panic!(
                                                    "Invalid expression in select clause: {:?}",
                                                    expr
                                                );
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            }

                            if !func_exists(FUNC.get().unwrap(), &name.to_string()) {
                                // TODO: add custom error
                                panic!("Function {} does not exist in functions", name.to_string());
                            }

                            if args.is_empty() {
                                // TODO: add custom error
                                panic!(
                                    "Function {} requires at least one argument",
                                    name.to_string()
                                );
                            }

                            table_cols.push(Column::FuncColumn(FuncColumn {
                                func_name: name.to_string(),
                                args,
                            }));
                        }
                        _ => {
                            // TODO: add custom error
                            panic!("Invalid expression in select clause: {:?}", expr);
                        }
                    }
                }
            }
            _ => {}
        }

        self.project_columns = table_cols;

        self
    }

    #[inline]
    fn with_where_clause(mut self, clause: &Clause) -> Self {
        assert!(matches!(clause, Clause::WhereClause(_)));

        let mut conditions: Vec<Condition> = vec![];
        match clause {
            Clause::WhereClause(exprs) => {
                conditions.extend(exprs.into_iter().map(|expr| expr.into()));
            }
            _ => {}
        }

        for cond in &conditions {
            match (&cond.left, &cond.right) {
                (CondVal::Column(ref left), CondVal::Column(ref right)) => {
                    let table_md = TABLE.get().unwrap();
                    if !columns_exists(table_md, &left.table_name, &left.column) {
                        // TODO: add custom error
                        panic!(
                            "Column {} does not exist in table {}",
                            left.column, left.table_name,
                        );
                    }

                    if !columns_exists(table_md, &right.table_name, &right.column) {
                        // TODO: add custom error
                        panic!(
                            "Column {} does not exist in tables {}",
                            right.column, right.table_name
                        );
                    }
                }
                (CondVal::Column(ref left), _) => {
                    let table_md = TABLE.get().unwrap();
                    if !columns_exists(table_md, &left.table_name, &left.column) {
                        // TODO: add custom error
                        panic!(
                            "Column {} does not exist in table {}",
                            left.column, left.table_name,
                        );
                    }
                }
                _ => {}
            }
        }

        self.filter_operators.push(Filter { conditions });

        self
    }

    #[inline]
    fn with_join_clause(mut self, clause: &Clause) -> Self {
        assert!(matches!(clause, Clause::FromClause { .. }));
        // select t1.name, t2.age from t1 left join t2 on t1.id = t2.uid;

        if let Clause::FromClause(left_table, Some(join_clause)) = clause {
            if let Clause::JoinClause {
                join_on,
                condition,
                join_type,
            } = join_clause.as_ref()
            {
                let left_table = left_table.to_string();
                let right_table = join_on.to_string();

                let table_md = TABLE.get().unwrap();

                if !table_exists(table_md, &left_table) {
                    // TODO: add custom error
                    panic!("Table {} does not exist", left_table);
                }

                if !table_exists(table_md, &right_table) {
                    // TODO: add custom error
                    panic!("Table {} does not exist", right_table);
                }

                let mut conditions: Vec<Condition> = vec![];
                conditions.extend(condition.into_iter().map(|expr| expr.into()));

                for cond in &conditions {
                    if let CondVal::Column(TableColumn { table_name, column }) = &cond.left
                        && !columns_exists(table_md, table_name, column)
                    {
                        // TODO: add custom error
                        panic!("Column {} does not exist in table {}", column, table_name)
                    }
                    if let CondVal::Column(TableColumn { table_name, column }) = &cond.right
                        && !columns_exists(table_md, table_name, column)
                    {
                        // TODO: add custom error
                        panic!("Column {} does not exist in table {}", column, table_name)
                    }
                }

                self.join_operator = Some(Join {
                    join_type: join_type.clone(),
                    left_table_name: left_table,
                    right_table_name: right_table,
                    condition: conditions,
                });
            }
        }

        self
    }

    #[inline]
    fn with_order_clause(mut self, clause: &Clause) -> Self {
        assert!(matches!(clause, Clause::OrderByClause(_)));

        match clause {
            Clause::OrderByClause(exprs) => {
                // TODO: use only one order now
                let expr = &exprs[0];
                let table_md = TABLE.get().unwrap();

                match &expr.0 {
                    ast::Expr::DotExpr(table, column) => {
                        if let (ast::Expr::IdentExpr(table), ast::Expr::IdentExpr(column)) =
                            (table.as_ref(), column.as_ref())
                        {
                            if !columns_exists(table_md, &table.to_string(), &column.to_string()) {
                                // TODO: add custom error
                                panic!(
                                    "Column {} does not exist in table {}",
                                    column.to_string(),
                                    table.to_string()
                                );
                            } else {
                                self.order_operator = Some(Order {
                                    column: TableColumn {
                                        table_name: table.to_string(),
                                        column: column.to_string(),
                                    },
                                    order: expr.1.clone(),
                                });
                            }
                        }
                    }
                    ast::Expr::IdentExpr(ident) => {
                        let mut founded = false;

                        for scan in &self.scan_operators {
                            if columns_exists(table_md, &scan.table_name, &ident.to_string()) {
                                if founded {
                                    // TODO: add custom error
                                    panic!("Column {} is ambiguous", ident.to_string());
                                } else {
                                    self.order_operator = Some(Order {
                                        column: TableColumn {
                                            table_name: scan.table_name.clone(),
                                            column: ident.to_string(),
                                        },
                                        order: expr.1.clone(),
                                    });
                                    founded = true;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }

        self
    }

    #[inline]
    fn with_group_by_clause(mut self, clause: &Clause) -> Self {
        assert!(matches!(clause, Clause::GroupByClause(_)));

        match clause {
            Clause::GroupByClause(exprs) => {
                // TODO: only support one group by column now
                let expr = &exprs[0];
                let table_md = TABLE.get().unwrap();

                match &expr {
                    ast::Expr::DotExpr(table, column) => {
                        if let (ast::Expr::IdentExpr(table), ast::Expr::IdentExpr(column)) =
                            (table.as_ref(), column.as_ref())
                        {
                            if !columns_exists(table_md, &table.to_string(), &column.to_string()) {
                                // TODO: add custom error
                                panic!(
                                    "Column {} does not exist in table {}",
                                    column.to_string(),
                                    table.to_string()
                                );
                            } else {
                                self.group_column = Some(GroupBy {
                                    column: TableColumn {
                                        table_name: table.to_string(),
                                        column: column.to_string(),
                                    },
                                    aggregate_fn: None,
                                    aggregate_column: None,
                                });
                            }
                        }
                    }
                    ast::Expr::IdentExpr(ident) => {
                        let mut founded = false;

                        for scan in &self.scan_operators {
                            if columns_exists(table_md, &scan.table_name, &ident.to_string()) {
                                if founded {
                                    // TODO: add custom error
                                    panic!("Column {} is ambiguous", ident.to_string());
                                } else {
                                    self.group_column = Some(GroupBy {
                                        column: TableColumn {
                                            table_name: scan.table_name.clone(),
                                            column: ident.to_string(),
                                        },
                                        aggregate_fn: None,
                                        aggregate_column: None,
                                    });
                                    founded = true;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }

        if self.group_column.is_some() {
            for column in &self.project_columns {
                if let Column::FuncColumn(func_col) = column {
                    let FuncColumn { func_name, args } = func_col;
                    if !FUNC
                        .get()
                        .unwrap()
                        .select(|func| func.func_name == *func_name && func.is_aggregate)
                        .is_empty()
                    {
                        self.group_column.as_mut().unwrap().aggregate_fn = Some(func_name.clone());
                        assert!(!args.is_empty());
                        self.group_column.as_mut().unwrap().aggregate_column =
                            Some(args[0].clone());
                    } else {
                        // TODO: add custom error
                        panic!("Function {} is not an aggregate function", func_name);
                    }
                }
            }
        }

        self
    }

    fn optimize(&mut self) {
        // TODO: add scan operator to join operator as children
    }
}

#[cfg(test)]
mod test {
    use std::{cell::RefCell, rc::Rc};

    use war_wolf_db_sql::{
        lexer::{token::Tokens, Lexer},
        parser::Parser,
    };

    use crate::query::QueryBuilder;

    use super::{
        operator::LogicOp,
        query_op::{Query, QueryOp, QueryType},
    };

    fn compare_input_with_query(input: &str, expected: Query) {
        let tokens = Lexer::lex(input).unwrap();
        let tokens = Tokens::new(&tokens);
        let ast = Parser::parse(tokens).unwrap();
        let query = QueryBuilder::new().with_stmt(&ast[0]).build();

        assert_eq!(query, expected);
    }

    #[test]
    fn test_query_builder() {
        // let input = "select t1.name, t2.age from t1;";
        // let expected = Query {
        //     query_type: QueryType::Select,
        //     root: Some(Rc::new(RefCell::new(QueryOp {
        //         data: LogicOp::Scan(super::Scan {
        //             table_name: "t1".to_string(),
        //             columns: vec![],
        //         }),
        //         next: None,
        //     }))),
        //     tail: None,
        //     size: 1,
        // };
        // compare_input_with_query(input, expected);
    }
}
