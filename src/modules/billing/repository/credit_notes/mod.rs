// Mongo and SQLite access for the `credit_notes` collection. Functions here never
// interpret a missing document as an error — they return `Option`/`Vec`/
// counts straight from the driver and leave the not-found -> AppError
// translation to `service`.

mod build_credit_note_where_clause;
mod count_credit_notes;
mod count_open_credit_notes_for_invoice;
mod credit_notes_collection;
mod find_credit_note_by_id;
mod find_credit_note_by_id_or_key;
mod find_credit_note_by_key;
mod insert_credit_note;
mod list_credit_notes;
mod update_credit_note_status;

pub(crate) use count_open_credit_notes_for_invoice::count_open_credit_notes_for_invoice;

pub(crate) use find_credit_note_by_id_or_key::find_credit_note_by_id_or_key;

pub(crate) use insert_credit_note::insert_credit_note;
pub(crate) use list_credit_notes::list_credit_notes;
pub(crate) use update_credit_note_status::update_credit_note_status;
