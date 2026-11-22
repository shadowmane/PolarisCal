-- Calibration certificates generated in PolarisCal: the number, who or what it is for,
-- and as JSON what was entered and the certificate template as it was when issued.
-- Users get an optional signature image (a data URL) printed on the certificates they sign.
CREATE TABLE certificates (
    id BIGINT IDENTITY(1,1) NOT NULL PRIMARY KEY,
    certificate_number NVARCHAR(64) NOT NULL,
    kind NVARCHAR(20) NOT NULL,
    asset_id BIGINT NULL,
    subject NVARCHAR(MAX) NOT NULL,
    description NVARCHAR(MAX) NOT NULL,
    cal_date DATE NOT NULL,
    details NVARCHAR(MAX) NOT NULL,
    template NVARCHAR(MAX) NOT NULL,
    issued_by_id BIGINT NULL,
    issued_by NVARCHAR(100) NOT NULL,
    created_at DATETIME2(7) NOT NULL,
    CONSTRAINT fk_certificates_asset_id FOREIGN KEY (asset_id) REFERENCES assets (id),
    CONSTRAINT fk_certificates_issued_by_id FOREIGN KEY (issued_by_id) REFERENCES users (id),
    CHECK (kind IN ('internal', 'external'))
);
CREATE UNIQUE INDEX ux_certificates_number ON certificates (certificate_number);
CREATE INDEX ix_certificates_asset_id ON certificates (asset_id);
ALTER TABLE users ADD signature NVARCHAR(MAX) NULL;
