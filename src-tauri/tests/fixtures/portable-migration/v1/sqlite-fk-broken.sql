PRAGMA foreign_keys = OFF;
INSERT INTO station_keys (id, station_id, label, secret_id, enabled, routing_enabled)
VALUES ('RPD_TEST_key_fk_broken', 'RPD_TEST_missing_station', 'RPD_TEST Broken FK', 'RPD_TEST_secret', 1, 1);
