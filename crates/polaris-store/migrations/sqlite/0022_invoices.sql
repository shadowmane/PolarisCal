-- Billing: the outside vendor invoice a service record was billed on, its line on that
-- invoice, and the invoice date. Saving an invoice also sets the record's cost to what the
-- invoice charged.
ALTER TABLE calibration_records ADD COLUMN invoice_number TEXT;
ALTER TABLE calibration_records ADD COLUMN invoice_line INTEGER;
ALTER TABLE calibration_records ADD COLUMN invoice_date TEXT;
CREATE INDEX ix_calibration_records_invoice ON calibration_records (provider_id, invoice_number);
