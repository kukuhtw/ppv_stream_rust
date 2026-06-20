# Federation Migration Rollback

## Migration 040 — Index-only federation foundation

To roll back `sql/040_federation_index_only.sql`:

```sql
-- 1. Remove columns added to existing tables
ALTER TABLE videos
    DROP COLUMN IF EXISTS object_uri,
    DROP COLUMN IF EXISTS federation_visibility,
    DROP COLUMN IF EXISTS federated_at,
    DROP COLUMN IF EXISTS federation_updated_at;

ALTER TABLE users
    DROP COLUMN IF EXISTS actor_uri,
    DROP COLUMN IF EXISTS federation_enabled,
    DROP COLUMN IF EXISTS discoverable;

-- 2. Drop federation-specific tables (order respects FK constraints)
DROP TABLE IF EXISTS federation_domain_rules;
DROP TABLE IF EXISTS remote_video_catalog;
DROP TABLE IF EXISTS federation_delivery_jobs;
DROP TABLE IF EXISTS federation_activities;
DROP TABLE IF EXISTS federation_follows;
DROP TABLE IF EXISTS federation_actors;
DROP TABLE IF EXISTS federation_instances;
```

## Migration 041 — Revenue sharing

To roll back `sql/041_federation_revenue.sql`:

```sql
DROP TABLE IF EXISTS revenue_ledger_entries;
DROP TABLE IF EXISTS federation_revenue_shares;
DROP TABLE IF EXISTS revenue_share_policies;
DROP TABLE IF EXISTS federation_referrals;
```

## Notes

* Roll back 041 before 040.
* If rows already exist in these tables, the `DROP TABLE` statements will
  delete all data.  Take a database snapshot before rolling back.
* The `sqlx` migration tracking table (`_sqlx_migrations`) should be
  cleared of the relevant rows after rollback so the migration is not
  marked as applied:

  ```sql
  DELETE FROM _sqlx_migrations WHERE description LIKE '%federation%';
  ```
