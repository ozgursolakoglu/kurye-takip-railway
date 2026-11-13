CREATE EXTENSION IF NOT EXISTS pgcrypto;
CREATE TABLE users (
 id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
 full_name text NOT NULL,
 email text NOT NULL UNIQUE,
 password_hash text NOT NULL,
 role text NOT NULL CHECK (role IN ('manager','courier')),
 active boolean NOT NULL DEFAULT true,
 created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE receipts (
 id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
 courier_id uuid NOT NULL REFERENCES users(id),
 receipt_number text NOT NULL,
 receipt_date date NOT NULL,
 payment_method text NOT NULL CHECK (payment_method IN ('cash','card','meal','iban')),
 status text NOT NULL DEFAULT 'pending' CHECK (status IN ('pending','approved','cancelled')),
 approved_by uuid REFERENCES users(id),
 approved_at timestamptz,
 cancelled_by uuid REFERENCES users(id),
 cancelled_at timestamptz,
 cancellation_reason text,
 created_at timestamptz NOT NULL DEFAULT now(),
 UNIQUE (receipt_date,receipt_number)
);
CREATE TABLE receipt_events (
 id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
 receipt_id uuid NOT NULL REFERENCES receipts(id),
 actor_id uuid NOT NULL REFERENCES users(id),
 action text NOT NULL CHECK (action IN ('created','approved','cancelled','reactivated')),
 reason text,
 created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX receipts_courier_date_idx ON receipts(courier_id,receipt_date);
CREATE INDEX receipt_events_receipt_idx ON receipt_events(receipt_id,created_at);
