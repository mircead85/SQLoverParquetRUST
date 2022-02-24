
pub mod shared {

    pub struct SqlSelectQuery {
        pub groupby_columns: Vec<String>, //If a sort column exists, it must be the first one in GroupBy. Enforced at parsing time.
        pub filter_columns: Vec<String>,
        pub filter_values: Vec<String>,
        pub orderby_column: Option<String>, //If None then OrderBy COUNT.
        pub orderby_order_asc: Option<bool>,
        pub limitrows: Option<u32>, //DEMO version assumes the limit is low enough for all rows to fit in memory.
        pub selected_columns: Vec<i32>, //DEMO version allows selecting only for what is in GroupBy columns. Value outside range implies outputing the Count statistic.
        pub selected_columns_alias: Vec<String>
    }

    pub fn say_hello(msg: String) {
        println!("{}", msg);
    }
}
