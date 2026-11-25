-- Service manuals, and a folder per kind of file. Files can be kept in separate folders
-- (Settings > Attachments folders); each row records which one it was saved in. Files
-- attached until now are all in the main (certificates) folder. File paths only need to be
-- unique within their folder.
-- The category check was created without a name, so look up the one SQL Server gave it.
DECLARE @ck sysname = (SELECT TOP 1 name FROM sys.check_constraints
                        WHERE parent_object_id = OBJECT_ID('attachments')
                          AND definition LIKE '%category%');
IF @ck IS NOT NULL EXEC('ALTER TABLE attachments DROP CONSTRAINT [' + @ck + ']');
GO
ALTER TABLE attachments ADD CONSTRAINT ck_attachments_category CHECK (category IN ('certificate', 'document', 'photo', 'service_manual', 'other'));
ALTER TABLE attachments ADD storage_folder NVARCHAR(20) NOT NULL
    CONSTRAINT df_attachments_storage_folder DEFAULT 'certificates';
GO
ALTER TABLE attachments ADD CONSTRAINT ck_attachments_storage_folder CHECK (storage_folder IN ('certificates', 'photos', 'service_manuals', 'documents'));
DROP INDEX ux_attachments_relative_path ON attachments;
CREATE UNIQUE INDEX ux_attachments_relative_path ON attachments (storage_folder, relative_path);
