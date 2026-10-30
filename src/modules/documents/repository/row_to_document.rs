use crate::clients::sqlite::to_bson_datetime;

use crate::modules::documents::model::GeneratedDocumentDocument;

use mongodb::bson::oid::ObjectId;
use sqlx::Row;

pub(super) fn row_to_document(r: &sqlx::sqlite::SqliteRow) -> GeneratedDocumentDocument {
    let id_str: String = r.get("id");
    let created_str: String = r.get("created_at");
    let updated_str: String = r.get("updated_at");
    GeneratedDocumentDocument {
        id: ObjectId::parse_str(&id_str).ok(),
        key: r.get("key"),
        entity_key: r.get("entity_key"),
        document_type: r.get("document_type"),
        template_name: r.get("template_name"),
        template_key: r.get("template_key"),
        template_ref: r.get("template_ref"),
        template_title: r.get("template_title"),
        template_version: r.get("template_version"),
        file_path: r.get("file_path"),
        file_size_bytes: r.get("file_size_bytes"),
        created_at: to_bson_datetime(&created_str),
        updated_at: to_bson_datetime(&updated_str),
    }
}
