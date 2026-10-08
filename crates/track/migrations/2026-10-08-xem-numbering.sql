-- A show's manual link from its episodes to XEM's numberings (api::Numbering as
-- JSON); NULL numbers episodes like TheTVDB.
ALTER TABLE shows ADD COLUMN numbering TEXT;
