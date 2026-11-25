-- Service manuals, and a folder per kind of file. Files can be kept in separate folders
-- (Settings > Attachments folders); each row records which one it was saved in. Files
-- attached until now are all in the main (certificates) folder. File paths only need to be
-- unique within their folder.
-- The category check was created without a name, so look up the one MySQL/MariaDB gave it.
SET @ck = (SELECT tc.CONSTRAINT_NAME
             FROM information_schema.TABLE_CONSTRAINTS tc
             JOIN information_schema.CHECK_CONSTRAINTS cc
               ON cc.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND cc.CONSTRAINT_NAME = tc.CONSTRAINT_NAME
            WHERE tc.TABLE_SCHEMA = DATABASE() AND tc.TABLE_NAME = 'attachments'
              AND tc.CONSTRAINT_TYPE = 'CHECK' AND cc.CHECK_CLAUSE LIKE '%category%'
            LIMIT 1);
SET @drop_ck = IF(@ck IS NULL, 'DO 0', CONCAT('ALTER TABLE attachments DROP CONSTRAINT `', @ck, '`'));
PREPARE drop_ck FROM @drop_ck;
EXECUTE drop_ck;
DEALLOCATE PREPARE drop_ck;
ALTER TABLE attachments ADD CONSTRAINT ck_attachments_category CHECK (category IN ('certificate', 'document', 'photo', 'service_manual', 'other'));
ALTER TABLE attachments ADD COLUMN storage_folder VARCHAR(20) NOT NULL DEFAULT 'certificates';
ALTER TABLE attachments ADD CONSTRAINT ck_attachments_storage_folder CHECK (storage_folder IN ('certificates', 'photos', 'service_manuals', 'documents'));
DROP INDEX ux_attachments_relative_path ON attachments;
CREATE UNIQUE INDEX ux_attachments_relative_path ON attachments (storage_folder, relative_path);
