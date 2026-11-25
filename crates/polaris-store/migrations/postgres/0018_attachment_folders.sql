-- Service manuals, and a folder per kind of file. Files can be kept in separate folders
-- (Settings > Attachments folders); each row records which one it was saved in. Files
-- attached until now are all in the main (certificates) folder. File paths only need to be
-- unique within their folder.
DO $$
DECLARE c text;
BEGIN
    FOR c IN
        SELECT conname FROM pg_constraint
         WHERE conrelid = 'attachments'::regclass AND contype = 'c'
           AND pg_get_constraintdef(oid) LIKE '%category%'
    LOOP
        EXECUTE format('ALTER TABLE attachments DROP CONSTRAINT %I', c);
    END LOOP;
END $$;
ALTER TABLE attachments ADD CONSTRAINT ck_attachments_category CHECK (category IN ('certificate', 'document', 'photo', 'service_manual', 'other'));
ALTER TABLE attachments ADD COLUMN storage_folder VARCHAR(20) NOT NULL DEFAULT 'certificates';
ALTER TABLE attachments ADD CONSTRAINT ck_attachments_storage_folder CHECK (storage_folder IN ('certificates', 'photos', 'service_manuals', 'documents'));
DROP INDEX ux_attachments_relative_path;
CREATE UNIQUE INDEX ux_attachments_relative_path ON attachments (storage_folder, LOWER(relative_path));
