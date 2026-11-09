-- 036_wallet_deposit_proof.sql
-- Deposit evidence: transfer reference, sender name and an uploaded proof
-- file (image/PDF) that the admin checks before approving a deposit.
-- Idempotent: safe to run more than once.

ALTER TABLE wallet_transactions
    ADD COLUMN IF NOT EXISTS transfer_ref   TEXT,
    ADD COLUMN IF NOT EXISTS sender_name    TEXT,
    ADD COLUMN IF NOT EXISTS proof_path     TEXT,
    ADD COLUMN IF NOT EXISTS proof_mime     TEXT,
    ADD COLUMN IF NOT EXISTS proof_uploaded_at TIMESTAMPTZ;
