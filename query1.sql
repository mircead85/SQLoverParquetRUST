SELECT "donors"."Donor State" "donors__donor_state",
    count(*) "donors__count" FROM 
    test.donors AS donors GROUP BY 
    1 
  ORDER BY 
    2 DESC 
  LIMIT 
    10000;
