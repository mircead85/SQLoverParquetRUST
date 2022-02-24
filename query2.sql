SELECT 
    count(*) 'donors__count' FROM 
    test.donors AS 'donors' WHERE 
    ("Donor City" = "San Francisco") 
  LIMIT 
    10000;
