ALTER TABLE tenants ADD COLUMN stripe_customer_id TEXT;

CREATE UNIQUE INDEX tenants_stripe_customer_id
    ON tenants (stripe_customer_id)
    WHERE stripe_customer_id IS NOT NULL;
