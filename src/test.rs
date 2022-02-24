pub mod test {
    use crate::dbstore::dbstore::convert_csv_to_parquet_file;
    use crate::webserver::webserver::get_result_for_query;

pub fn test_all() {
    crate::dbstore::dbstore::print_hello();
    crate::webserver::webserver::print_hello();
  
    let sql1 = "SELECT \"donors\".\"Donor State\" \"donors__donor_state\",\
    count(*) \"donors__count\" FROM \
    test.donors AS donors GROUP BY \
    1 \
  ORDER BY \
    2 DESC \
  LIMIT \
    10000;";
  
    let sql2 = "SELECT \
    count(*) 'donors__count' FROM \
    test.donors AS 'donors' WHERE \
    (\"Donor City\" = \"San Francisco\") \
  LIMIT \
    10000;";
  
    let sql3 = "SELECT \"donors\".\"Donor State\" \"donors__donor_state\",\
    count(*) \"donors__count\" FROM \
    test.donors AS donors \
    WHERE \
    (\"Donor Is Teacher\" = \"Yes\") AND (\"Donor City\" = \"San Francisco\") \
    GROUP BY \
    1 \
  ORDER BY \
    2 DESC \
  LIMIT \
    5;";
  
    
    let sql4 = "SELECT \"donors\".\"Donor State\" \"donors__donor_state\",\
    count(*) \"donors__count\" FROM \
    test.donors AS donors \
    WHERE \
    (\"Donor Is Teacher\" = \"Yes\")\
    GROUP BY \
    1 \
  ORDER BY \
    1 ASC \
  LIMIT \
    3;";
  
    let temp_path = String::from("Donors.csv");
    let table_path = String::from("Donors.parquet");
  
    convert_csv_to_parquet_file(&temp_path, 10_000, &table_path );
    let res2 = get_result_for_query(2, &String::from(sql2)).unwrap();
    println!("Query Results: {}",res2);
    let res1 = get_result_for_query(1, &String::from(sql1)).unwrap();
    println!("Query Results: {}",res1);
    let res3 = get_result_for_query(3, &String::from(sql3)).unwrap();
    println!("Query Results: {}",res3);
    let res4 = get_result_for_query(4, &String::from(sql4)).unwrap();
    println!("Query Results: {}",res4);
  }
}