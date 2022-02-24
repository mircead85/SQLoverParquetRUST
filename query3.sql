SELECT "donors"."Donor State" "donors__donor_state",
    count(*) "donors__count" FROM 
    test.donors AS donors 
    WHERE 
    ("Donor Is Teacher" = "Yes") AND ("Donor City" = "San Francisco") 
    GROUP BY 
    1 
  ORDER BY 
    2 DESC 
  LIMIT 
    5;
