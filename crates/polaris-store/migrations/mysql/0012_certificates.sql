-- Calibration certificates generated in PolarisCal: the number, who or what it is for,
-- and as JSON what was entered and the certificate template as it was when issued.
-- Users get an optional signature image (a data URL) printed on the certificates they sign.
CREATE TABLE certificates (
    id BIGINT NOT NULL AUTO_INCREMENT PRIMARY KEY,
    certificate_number VARCHAR(64) NOT NULL,
    kind VARCHAR(20) NOT NULL,
    asset_id BIGINT NULL,
    subject MEDIUMTEXT NOT NULL,
    description MEDIUMTEXT NOT NULL,
    cal_date DATE NOT NULL,
    details MEDIUMTEXT NOT NULL,
    template MEDIUMTEXT NOT NULL,
    issued_by_id BIGINT NULL,
    issued_by VARCHAR(100) NOT NULL,
    created_at DATETIME(6) NOT NULL,
    CONSTRAINT fk_certificates_asset_id FOREIGN KEY (asset_id) REFERENCES assets (id),
    CONSTRAINT fk_certificates_issued_by_id FOREIGN KEY (issued_by_id) REFERENCES users (id),
    CHECK (kind IN ('internal', 'external'))
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
CREATE UNIQUE INDEX ux_certificates_number ON certificates (certificate_number);
CREATE INDEX ix_certificates_asset_id ON certificates (asset_id);
ALTER TABLE users ADD COLUMN signature MEDIUMTEXT NULL;
