CREATE TRIGGER injected_trigger AFTER INSERT ON settings BEGIN SELECT 1; END;
CREATE VIEW injected_view AS SELECT 1 AS value;
