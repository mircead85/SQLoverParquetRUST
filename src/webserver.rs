pub mod webserver {
    use std::fs::File;
    use std::io::{Write, Read};
    use std::{panic};
    use crate::dbstore::dbstore::{convert_csv_to_parquet_file, solve_query};
    use crate::shared::shared::*;
    
    use nickel::status::StatusCode;
    use nickel::{Nickel, HttpRouter, QueryString, Request};

    use sqlparser::ast::{Statement, SetExpr, TableFactor, ObjectName, SelectItem, Expr, BinaryOperator};
    use sqlparser::dialect::*;
    use sqlparser::parser::*;

    static mut CUR_QUERY_ID:u32 = 100u32;

    pub fn start_webserver() {

        let mut server = Nickel::with_data("CubeDB".to_string());
        let mut router = Nickel::router();

        router.post("/table/:table", middleware! { |request, mut response| <String> 
        let tablename:String = request.param("table").unwrap().to_string();

        match get_result_for_post_table_streaming(&tablename, request) {
            Ok(rez_str) => (StatusCode::Ok, rez_str),
            Err(err) => {response.set(StatusCode::InternalServerError); return response.send(err.to_string());}
        }
        });

        router.get("/query", middleware! { |request, mut response| <String> 
            let query_str:String = request.query().get("sql").unwrap_or_default().to_string();
            unsafe {CUR_QUERY_ID+=1;} //Should be done with some middleware better
            unsafe {
            match get_result_for_query(CUR_QUERY_ID, &query_str) {
                Ok(rez_str) => (StatusCode::Ok, rez_str),
                Err(err) => (StatusCode::InternalServerError, err.to_string())
            }
            }
        });

        server.utilize(router);

        server.listen("127.0.0.1:9000").unwrap();
    }

    pub fn get_result_for_post_table_streaming(table_name: &String, request: &mut Request<String>) -> Result<String, String> {
            let temp_path = String::from("t_")+table_name+".csv.tmp";
            let table_path: String = String::from("t_")+table_name+".parquet";
            let mut csv_file = File::create(&temp_path).unwrap();
            
            let mut buf = [0u8;1_200_000];

            loop {
                let read = (*request).origin.read(&mut buf).unwrap();
                if read == 0 {
                    break;
                }
                csv_file.write_all(&mut buf[0..read]).unwrap();
            }
            
            csv_file.flush().unwrap();

            convert_csv_to_parquet_file(&temp_path, 50_000, &table_path );

            return Ok("Ok. Table Created / Refreshed.".to_string());
    }

    pub fn get_result_for_post_table(table_name: &String, post_content: &String) -> Result<String, String> {
        let post_result = panic::catch_unwind( || {
            let temp_path = String::from("t_")+table_name+".csv.tmp";
            let table_path: String = String::from("t_")+table_name+".parquet";
            let mut csv_file = File::create(&temp_path).unwrap();
            csv_file.write_all(post_content.as_bytes()).unwrap();
            csv_file.flush().unwrap();

            convert_csv_to_parquet_file(&temp_path, 50_000, &table_path );

            return String::from("Ok. Table Created / Refreshed.");
        });

        match post_result {
            Ok(result_str) => {return Ok(result_str);}
            Err(err) => {return Err(*err.downcast::<String>().unwrap());}
        }
    }

    pub fn get_result_for_query(query_id: u32, query_str: &String) -> Result<String, String> {
        let get_result = panic::catch_unwind( || {
            let (sql_query, table_name) = parse_sql_query(query_str).unwrap();
            let query_results_path = String::from("q_")+&query_id.to_string()+"_result.csv";
            
            solve_query(query_id, sql_query, &table_name);

            let mut result_as_str = String::new();
            let mut result_file = File::open(query_results_path).unwrap();
            result_file.read_to_string(&mut result_as_str).unwrap();

            return result_as_str; //Not yet sure how strings work in Rust.. will this copy the string or just the reference? In C# it would be just the reference.
        });

        match get_result {
            Ok(result_str) => {return Ok(result_str);}
            Err(err) => {return Err(*err.downcast::<String>().unwrap());}
        }
    }

    pub fn parse_sql_query(sql_query: &String) -> Result<(SqlSelectQuery, String), String> {
        let parsing_result = panic::catch_unwind( || {
            let dialect = GenericDialect {};
            let ast = Parser::parse_sql(&dialect, &sql_query).unwrap();
            let mut result: SqlSelectQuery = SqlSelectQuery{
                groupby_columns: vec![],
                filter_columns: vec![],
                filter_values: vec![],
                orderby_column: None,
                orderby_order_asc: None,
                selected_columns: vec![],
                selected_columns_alias: vec![],
                limitrows: Some(1000_000u32)
            };
            let mut table_name: String = String::from("");

            if let Statement::Query(q1) = &ast[0] {
                if let SetExpr::Select(q2) = &q1.body {
                    if let TableFactor::Table{name: ObjectName(q3), alias:q4,..} = &(q2.from[0].relation) {
                        table_name = q3[q3.len()-1].to_string();
                        //Ignore alias.
                        println!("TableName: {}. Alias: {}.", q3[q3.len()-1].to_string(), q4.as_ref().unwrap().to_string());
                    } else {
                        panic!("Unrecognized way of specifying FROM table name in {}", q2);
                    }
                    
                    let mut selected_column_aliases:Vec<String> = vec![];
                    let mut selected_column:Vec<String> = vec![];
                    let count_str = String::from("COUNT");
    
                    for q8 in &q2.projection {
                        match &q8 {
                            SelectItem::ExprWithAlias{expr: q9, alias: q11} => {
                                if let Expr::Identifier(q10) = &q9 {
                                    selected_column.push(q10.value.clone());
                                } else if let Expr::CompoundIdentifier(q10)= &q9 {
                                    selected_column.push(q10[q10.len()-1].value.clone());
                                } else if let Expr::Function(q10)=&q9 {
                                    if q10.name.to_string().to_uppercase()==count_str {
                                        selected_column.push(count_str.clone());
                                    }
                                    else {
                                        panic!("Unexpected select function: {}. Only COUNT supported.", q10);
                                    }
                                } else {
                                    panic!("Unexpected select item: {}", q8);
                                }
                                selected_column_aliases.push(q11.value.clone());
                                println!("Select Column: {} with alias {}.", selected_column[selected_column.len()-1],
                                        selected_column_aliases[selected_column_aliases.len()-1]);
                            }
                            SelectItem::UnnamedExpr(q9) => { //Known issue: Semantically duplicate code.
                                if let Expr::Identifier(q10) = &q9 {
                                    selected_column.push(q10.value.clone());
                                    selected_column_aliases.push(q10.value.clone());
                                } else if let Expr::CompoundIdentifier(q10)= &q9 {
                                    selected_column.push(q10[q10.len()-1].value.clone());
                                    selected_column_aliases.push(q10[q10.len()-1].value.clone());
                                } else if let Expr::Function(q10)=&q9 {
                                    if q10.name.to_string().to_uppercase()==count_str {
                                        selected_column.push(count_str.clone());
                                        selected_column_aliases.push(count_str.clone());
                                    }
                                    else {
                                        panic!("Unexpected select function: {}. Only COUNT supported.", q10);
                                    }
                                } else {
                                    panic!("Unexpected select item: {}", q8);
                                }
                                
                                println!("Select Column: {} with alias {}.", selected_column[selected_column.len()-1],
                                        selected_column_aliases[selected_column_aliases.len()-1]);
                            }
                            _ => panic!("Unexpected projection term {}", q8)
                        }
                    }
    
                    let mut groupby_columns:Vec<String> = vec![];

                    for q5 in &q2.group_by {
                        if let Expr::Identifier(q6) = &q5 {
                        groupby_columns.push(q6.value.clone());
                        println!("GrouBy: {}",q6.value);
                        }
                        else {
                            let q7_r = q5.to_string().trim().parse::<u32>();
                            let q7 = q7_r.unwrap() as usize;
                            groupby_columns.push(selected_column[q7-1].clone());
                            println!("GroupBy from Select Column #{}: {}.",q7.to_string(), selected_column[q7-1]);
                        }
                    }
                    let mut filter_columns: Vec<String> = vec![];
                    let mut filter_values: Vec<String> = vec![];

                    if q2.selection != None {
                        extract_filter_conds(q2.selection.as_ref().unwrap(), &mut filter_columns, &mut filter_values);
                    }

                    result.groupby_columns = groupby_columns.clone();
                    result.filter_columns = filter_columns.clone();
                    result.filter_values = filter_values.clone();
                    result.selected_columns_alias = selected_column_aliases.clone();
                    for i in 0..selected_column.len() {
                        if selected_column[i]==count_str {
                            result.selected_columns.push(-1);
                            continue;
                        }
                        let mut b_found = false;
                        for gbidx in 0..groupby_columns.len() {
                            if groupby_columns[gbidx] == selected_column[i] {
                                result.selected_columns.push(gbidx as i32);
                                b_found = true;
                                break;
                            }
                        }
                        if !b_found {
                            panic!("Cannot select in the DEMO for something that is not in the GROUPBY statement or is the COUNT function.");
                        }
                    }
                }
                if q1.order_by.len()>0 {
                    if let Expr::Identifier(q19) = &q1.order_by[0].expr {
                            result.orderby_column = Some(q19.value.clone());
                            println!("OrderBy: {}", &q19.value);
                        } else if let Expr::CompoundIdentifier(q19)= &q1.order_by[0].expr {
                            result.orderby_column = Some(q19[q19.len()-1].value.clone());
                            println!("OrderBy: {}", &q19[q19.len()-1].value);
                        }
                        else {
                            let col_id = q1.order_by[0].expr.to_string().parse::<usize>();
                            if let Ok(ob_colid) = col_id {
                                if result.selected_columns[ob_colid-1] == -1 {
                                    result.orderby_column = None;
                                } else {
                                    result.orderby_column = Some(result.groupby_columns[result.selected_columns[ob_colid-1] as usize].clone());
                                }
                            } else {
                                panic!("Unexpected order by clause: {}. In DEMO version you can only orderby columns, not by expressions, including not by COUNT.", q1.order_by[0]);
                            }
                        }
                    result.orderby_order_asc = Some(true);
                    if let Some(false) = q1.order_by[0].asc {
                        result.orderby_order_asc = Some(false);
                        println!("OrderBy DESC.");
                    } else {
                        result.orderby_order_asc = Some(true);
                        println!("OrderBy ASC.");
                    }
                    if let None = result.orderby_column {
                        println!("OrderBy COUNT.");
                    }
                    else {
                        if !result.groupby_columns.contains(&result.orderby_column.as_ref().unwrap()) {
                            let mut b_ok = false;
                            for col_id in 0..result.selected_columns_alias.len() {
                                if result.selected_columns_alias[col_id].to_uppercase() == (&result.orderby_column.as_ref().unwrap()).to_uppercase() {
                                    b_ok = true;
                                    result.orderby_column = Some(result.groupby_columns[result.selected_columns[col_id] as usize].clone());
                                }
                            }
                            if !b_ok {
                                panic!("Unexpected OrderBy column name.");
                            }
                        }
                        if q1.order_by.len()>1 {
                            panic!("Cannot order by more than 1 column in DEMO version.");
                        }
                    }
                }

                if let Some(q18) = &q1.limit {
                    let limit_str = q18.to_string().to_uppercase();
                    if limit_str == "ALL" {
                        result.limitrows = None;
                        println!("LIMIT ALL");
                    } else {
                        result.limitrows = Some(limit_str.parse::<u32>().unwrap());
                        println!("LIMIT {}", &result.limitrows.unwrap())
                    }
                }
            }

            return (result, table_name);
        });

        match parsing_result {
            Ok(res) => {return Ok(res);}
            Err(err) => {return Err(*err.downcast::<String>().unwrap());}
        }
    }

    fn extract_filter_conds(q2: &Expr, filter_columns: &mut Vec<String>, filter_values: &mut Vec<String>) {
        let mut q12 = q2;
        let mut q14 = q12;
        loop {
            if let Expr::Nested(q13) = q14 {
                    q14 = q13;
            } else {
                q12 = q14;
                break;
            }
        }

        if let Expr::BinaryOp{left: q15, op: q16, right: q17} = q12 {
            if let BinaryOperator::And = q16 {
                extract_filter_conds(q15, filter_columns, filter_values);
                extract_filter_conds(q17, filter_columns, filter_values);
            } else if let BinaryOperator::Eq = q16 {
                if let Expr::Identifier(q6) = &**q15 {
                    filter_columns.push(q6.value.clone());
                    println!("FilterBy Column: {}.",q6.value);
                    }
                else if let Expr::CompoundIdentifier(q6) = &**q15 {
                    filter_columns.push(q6[q6.len()-1].value.clone());
                    println!("FilterBy Column: {}.",q6[q6.len()-1].value);
                    }
                else {
                    panic!("Unexpected expression {}. Only simple equals filters column=value are supported in DEMO version.", &q12);
                }

                if let Expr::Value(q7) =  &**q17 {
                    filter_values.push(q7.to_string().clone());
                    println!("FilterBy Value: {}.", q7.to_string());
                    }
                else if let Expr::Identifier(q7) = &**q17 {
                    filter_values.push(q7.value.clone());
                    println!("FilterBy Value: {}.",q7.value);
                    }
                else {
                    panic!("Unexpected expression {}. Only simple equals filters column=value are supported in DEMO version.", &q12);
                }
            }
        }
    }

    pub fn print_hello() {
        say_hello("Hello from webserver module!".to_string());
    }
}
