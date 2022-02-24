pub mod dbstore {
    use crate::shared::shared::*;
    use std::collections::BinaryHeap;
    use std::io::Write;
    use std::{fs::File, collections::HashMap};
    use std::path::Path;
    use arrow::csv::{ReaderBuilder};
    //use arrow2::io::parquet; //<-- On my machine I get big error "parquet not part of arrow2::io". Maybe my download URL is missconfigured to something old.
    
    use parquet::arrow::ArrowWriter;
    use parquet::file::properties::WriterProperties;
    use parquet::record::{Row, RowAccessor};
    use parquet::{file::{reader::{FileReader, SerializedFileReader}}, schema::types::Type};

    pub fn solve_query(query_id: u32, sql_query: SqlSelectQuery, tablename: &String) -> String {
        let parquet_file_path = String::from("t_") + tablename + ".parquet";
        let query_output_file_path = String::from("q_")+&query_id.to_string()+"_result.csv";
        process_table_file_parquet(&parquet_file_path, sql_query, &query_output_file_path);
        return query_output_file_path;
    }

    pub fn process_table_file_parquet(table_file_path: &String, query: SqlSelectQuery, output_file_path: &String) {
        let input_file = File::open(&Path::new(table_file_path)).unwrap();
        let mut file_reader = SerializedFileReader::new(input_file).unwrap();
        let file_metadata = file_reader.metadata();

        // Projection Pushdown
        let all_columns = file_metadata.file_metadata().schema().get_fields(); 
        let mut selected_columns = all_columns.to_vec();
        selected_columns.retain(|f|  
                query.filter_columns.contains(&String::from(f.name())) 
                 || query.groupby_columns.contains(&String::from(f.name()))
                 || query.orderby_column.eq(&Some(String::from(f.name())))
            );
        
        let schema_projection = Type::group_type_builder(file_metadata.file_metadata().schema().get_basic_info().name())
            .with_fields(&mut selected_columns)
            .build()
            .unwrap();


        //Setup
        let filter_values_bytes = query.filter_values.iter().map(|v| v.as_bytes()).collect::<Vec<&[u8]>>();
        let mut groupbys_heap_asc: BinaryHeap<HeapElemAsc> = BinaryHeap::new();
        let mut groupby_res_map: HashMap<Vec<String>, u32> = HashMap::new();
        let mut effective_limit = query.limitrows.unwrap_or_else(|| 1_000_000u32); //No more than 1M grouby rows in total in this DEMO project
        let mut heap_len = 0u32;
        let mut heap_asc = false;

        let mut schema_groupby_mapper: Vec<usize> = vec![];
        let mut schema_filter_mapper: Vec<usize> = vec![];
        let mut schema_orderby_mapper: Vec<usize> = vec![];

        for j in 0..query.groupby_columns.len() {
            for i in 0..all_columns.len(){
                if schema_projection.get_fields()[i].name().to_string() == query.groupby_columns[j]  {
                    schema_groupby_mapper.push(i);
                    break;
                } 
            }
        }
        
        for j in 0..query.filter_columns.len() {
            for i in 0..all_columns.len(){
                if schema_projection.get_fields()[i].name().to_string() == query.filter_columns[j]  {
                    schema_filter_mapper.push(i);
                    break;
                } 
            }
        }

        if let None = query.orderby_column {

        } else {
            for i in 0..all_columns.len(){
                    if schema_projection.get_fields()[i].name().to_string() == *query.orderby_column.as_ref().unwrap()  {
                        schema_orderby_mapper.push(i);
                        break;
                    }
                }
        }

        if let Some(true) = query.orderby_order_asc {
            heap_asc = true;
        } 

        if let (None, Some(_)) = (&query.orderby_column, &query.orderby_order_asc) {
            effective_limit = 4_000_000_000; //Need to process all groups to know which one comes first in sort order. Sad.
        }

        //Filter RowGroups Predicate Setup
        file_reader.filter_row_groups(&|rowgroup_meta, _| 
            {
                for column_meta in rowgroup_meta.columns() {
                    if let Some(rg_column_stats) = column_meta.statistics() {
                        if !rg_column_stats.has_min_max_set() {
                            continue;
                        }
                        let rg_column_name = column_meta.column_descr().path().to_string().to_uppercase();
                        for fcol_idx in 0..query.filter_columns.len() { //Yeah, this could be optimized, but total number of columns is expected to be less than ~10 and the runtime of doing this in O(n^2) is negligible to the rest CPU time
                            let filter_col_name = &(&query.filter_columns)[fcol_idx];
                            if !filter_col_name.to_uppercase().eq(&rg_column_name) {
                                continue;
                            }
                            let rg_column_max = rg_column_stats.max_bytes();
                            let rg_column_min = rg_column_stats.min_bytes();
                            let filter_col_value = filter_values_bytes[fcol_idx];

                            if compare_bytes_less_than(filter_col_value, rg_column_min) {
                                return false;
                            }

                            if compare_bytes_less_than(rg_column_max, filter_col_value) {
                                return false;
                            }
                        }
                    }
                }
                return true;
            }
        );

        //Scan file
        let mut row_iter = file_reader.get_row_iter(Some(schema_projection)).unwrap();

        while let Some(record) = row_iter.next() {
            //Apply filers
            let actual_filter_values = extract_col_values(&record, false, &schema_filter_mapper, &schema_orderby_mapper);
            let mut record_filter_ok = true;

            for fidx in 0..query.filter_values.len() {
                if query.filter_values[fidx]!=*actual_filter_values[fidx] && String::from("\"") + &query.filter_values[fidx] + "\""!=*actual_filter_values[fidx] {
                    record_filter_ok = false;
                    break;
                }
            }
            if !record_filter_ok {
                //row_iter.skip_ahead_to_this_column_value_for_column(query.filter_columns[fidx], query.filter_values[fidx]);
                //^-- should be implementable to work fast given parquet format with dictionary (?). 
                //^-- But out of scope of current DEMO project.
                continue;
            }
            
            
            //Update GroupBy rows and statistics
            let record_values = extract_col_values(&record, true, &schema_groupby_mapper, &schema_orderby_mapper);
            let mut record_values_durable: Vec<String> = vec![];
            for r_val in *record_values {
                record_values_durable.push((*r_val).to_owned());
            }

            if let Some(cur_sum_entry) = groupby_res_map.get_mut(&record_values_durable) {
                *cur_sum_entry+=1;
            } else {
                groupby_res_map.insert(record_values_durable.clone(), 1);
                groupbys_heap_asc.push(HeapElemAsc {fields: record_values_durable.clone(), asc: heap_asc, count:None});
                heap_len+=1;

                if (heap_len as u32) > effective_limit {
                    let deleted_row = groupbys_heap_asc.pop().unwrap().fields;
                    heap_len-=1;
                    groupby_res_map.remove(&deleted_row);
                }
            }
        }

        //Translate elements to sorted order (only required when sorting by COUNT)
        let mut rez :Vec<HeapElemAsc> = vec![];
        heap_asc = !heap_asc;
        
        groupbys_heap_asc.clear(); //free up memory. heap no longer required.

        for (group_values, count_value) in groupby_res_map.iter() {
            let heap_elem = HeapElemAsc {
                asc: heap_asc,
                count: {
                    if let (None, Some(_)) = (&query.orderby_column, &query.orderby_order_asc) {
                        Some(*count_value)
                    } else {
                        None
                    }
                },
                fields: group_values.clone()
            };
            rez.push(heap_elem);
        }

        if let None = &query.orderby_order_asc {
        } else {
            rez.sort(); //Actually with non-COUNT OrderBy, sorting would not be required - but this feature is not implemented in this DEMO version.
            rez.reverse();
        }
       
        //Write output file
        let mut output_file = File::create(output_file_path).unwrap();
        let mut written_rows = 0u32;
        let comma_str = String::from(",");
        let newline_str: String = String::from("\n");

        //Output header
        for i in 0..query.selected_columns.len() {
            let col_name = &query.selected_columns_alias[i];

            output_file.write(col_name.as_bytes()).unwrap();

            if i < query.selected_columns.len()-1 {
                output_file.write(&comma_str.as_bytes()).unwrap();
            }
        }
        output_file.write(&newline_str.as_bytes()).unwrap();

        //Output data
        for popped_row in rez {
            let cur_count_str = groupby_res_map.get(&popped_row.fields).unwrap().to_string();

            for i in 0..query.selected_columns.len() {
                let col_id = query.selected_columns[i];
                let col_value = {if col_id<0 || col_id>=(query.groupby_columns.len() as i32) {&cur_count_str} else {&popped_row.fields[col_id as usize]}};

                output_file.write(col_value.as_bytes()).unwrap();

                if i < query.selected_columns.len()-1 {
                    output_file.write(&comma_str.as_bytes()).unwrap();
                }
            }
            output_file.write(&newline_str.as_bytes()).unwrap();
            written_rows+=1;
            if let Some(query_limit_rows) = &query.limitrows {
                if written_rows >= *query_limit_rows {
                    break;
                }
            }
        }
        output_file.flush().unwrap();
    }

    //Method content inspired by https://github.com/domoritz/csv2parquet/blob/main/src/main.rs.
    pub fn convert_csv_to_parquet_file(csv_file_path: &String, max_rowgroup_size: usize, output_file_path: &String) {
        let input_file = File::open(&Path::new(csv_file_path)).unwrap();

        let mut builder = ReaderBuilder::new()
            .has_header(true)
            .with_batch_size(50_000)
            .with_delimiter(',' as u8);
        
        builder = builder.infer_schema(Some(0));

        let input_reader = builder.build(input_file).unwrap();
        
        let output_file = File::create(output_file_path).unwrap();
        let writer_props = WriterProperties::builder()
            .set_dictionary_enabled(false)
            .set_statistics_enabled(true)
            .set_max_row_group_size(max_rowgroup_size)
            .set_compression(parquet::basic::Compression::SNAPPY)
            .build();
        let mut output_writer = ArrowWriter::try_new(output_file, input_reader.schema(), Some(writer_props)).unwrap();

        //TOOD: Ommitted as out of scope for DEMO project - build the parquet file so that it can efficiently answer queries, not bluntly.

        for read_batch in input_reader {
            output_writer.write(&read_batch.unwrap()).unwrap();
        }

        output_writer.close().unwrap();
    }


    fn compare_bytes_less_than(left: &[u8], right: &[u8]) -> bool {
        for i in 0..left.len() {
            if i>=right.len() {return false;}
            
            if right[i] < left[i] {return false;}
            if left[i] < right[i] {return true;}
        }

        return true;
    }
    
    fn extract_col_values<'a>(record: &'a Row, orderby_column: bool, col_mapper: &Vec<usize>, orderby_mapper: &Vec<usize>) -> Box<Vec<&'a String>> {
        let mut result_values: Box<Vec<&'a String>> = Box::new(vec![]);
        if orderby_column {
            if orderby_mapper.len()>0 {
                result_values.push(record.get_string(orderby_mapper[0]).unwrap()); //I am afraid to_string() copies here instead of referencing.. but time is what it is and working with object lifetimes in Rust is still something for me to look into.
            }
        }
        for col_idx in col_mapper {
            result_values.push(record.get_string(*col_idx).unwrap());
        }
        return result_values;
    }
    
    struct HeapElemAsc {
        pub count: Option<u32>,
        pub asc: bool,
        pub fields: Vec<String>
    }

    impl PartialEq for HeapElemAsc {
        fn eq(&self, other: &Self) -> bool {
            self.count.eq(&other.count) && self.asc.eq(&other.asc) && self.fields.cmp(&other.fields)==std::cmp::Ordering::Equal
        }
    }

    impl PartialOrd for HeapElemAsc {
        //Basical copy/past from Ord below.. Did not figure out how to call that code yet from here.
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            let ord_res: std::cmp::Ordering;
            if let Some(cnt) = self.count {
                if self.asc  {
                    ord_res = cnt.cmp(&other.count.unwrap());
                } else {
                    ord_res = other.count.unwrap().cmp(&cnt);
                }

            } else {
                if self.asc  {
                    ord_res = self.fields.cmp(&other.fields);
                } else {
                    ord_res = other.fields.cmp(&self.fields);
                }
            }

            return Some(ord_res);
        }
    }

    impl Eq for HeapElemAsc {
        
    }

    impl Ord for HeapElemAsc {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            let ord_res: std::cmp::Ordering;
            if let Some(cnt) = self.count {
                if self.asc  {
                    ord_res = cnt.cmp(&other.count.unwrap());
                } else {
                    ord_res = other.count.unwrap().cmp(&cnt);
                }

            } else {
                if self.asc  {
                    ord_res = self.fields.cmp(&other.fields);
                } else {
                    ord_res = other.fields.cmp(&self.fields);
                }
            }

            return ord_res;
        }
    }

    pub fn print_hello() {
        say_hello("Hello from dbstore module!".to_string());
    }
}
