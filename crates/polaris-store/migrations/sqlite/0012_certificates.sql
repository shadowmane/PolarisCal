-- Calibration certificates generated in PolarisCal: the number, who or what it is for,
-- and as JSON what was entered and the certificate template as it was when issued.
-- Users get an optional signature image (a data URL) printed on the certificates they sign.
CREATE TABLE certificates (
    id                 INTEGER PRIMARY KEY,
    certificate_number TEXT    NOT NULL,
    kind               TEXT    NOT NULL CHECK (kind IN ('internal', 'external')),
    asset_id           INTEGER REFERENCES assets (id),
    subject            TEXT    NOT NULL,
    description        TEXT    NOT NULL,
    cal_date           TEXT    NOT NULL,
    details            TEXT    NOT NULL,
    template           TEXT    NOT NULL,
    issued_by_id       INTEGER REFERENCES users (id),
    issued_by          TEXT    NOT NULL,
    created_at         TEXT    NOT NULL
);
CREATE UNIQUE INDEX ux_certificates_number ON certificates (certificate_number COLLATE NOCASE);
CREATE INDEX ix_certificates_asset_id ON certificates (asset_id);
ALTER TABLE users ADD COLUMN signature TEXT;
