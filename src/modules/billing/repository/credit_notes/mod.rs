//! Mongo and SQLite access for the `credit_notes` collection. Functions here never
//! interpret a missing document as an error — they return `Option`/`Vec`/
//! counts straight from the driver and leave the not-found -> AppError
//! translation to `service`.

mod repository_credit_notes;

pub(crate) use repository_credit_notes::count_open_credit_notes_for_invoice;

pub(crate) use repository_credit_notes::find_credit_note_by_id_or_key;

pub(crate) use repository_credit_notes::insert_credit_note;
pub(crate) use repository_credit_notes::list_credit_notes;
pub(crate) use repository_credit_notes::update_credit_note_status;
