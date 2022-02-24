SELECT "donors"."Donor State" "donors__donor_state",
    count(*) "donors__count" FROM 
    test.donors AS donors 
    WHERE 
    ("Donor Is Teacher" = "Yes")
    GROUP BY 
    1 
  ORDER BY 
    1 ASC 
  LIMIT 
    3;
