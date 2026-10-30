use crate::clients::sqlite::SqlFilter;
use crate::clients::sqlite::bson_to_iso;

use mongodb::bson::Document;

pub(super) fn build_credit_note_where_clause(filter: &Document) -> SqlFilter {
    let mut conditions = vec!["deleted_at IS NULL".to_string()];
    let mut bindings = Vec::new();

    fn process_clause(doc: &Document, conditions: &mut Vec<String>, bindings: &mut Vec<String>) {
        if let Ok(key) = doc.get_str("key") {
            conditions.push("key = ?".to_string());
            bindings.push(key.to_string());
        }
        if let Ok(inv_key) = doc.get_str("invoice_key") {
            conditions.push("invoice_key = ?".to_string());
            bindings.push(inv_key.to_string());
        }
        if let Ok(cust_key) = doc.get_str("customer_key") {
            conditions.push("customer_key = ?".to_string());
            bindings.push(cust_key.to_string());
        }
        if let Ok(status) = doc.get_str("status") {
            conditions.push("status = ?".to_string());
            bindings.push(status.to_string());
        } else if let Ok(status_doc) = doc.get_document("status")
            && let Ok(ne) = status_doc.get_str("$ne")
        {
            conditions.push("status != ?".to_string());
            bindings.push(ne.to_string());
        }
        if let Ok(created_at_doc) = doc.get_document("created_at") {
            if let Ok(gte) = created_at_doc.get_datetime("$gte") {
                conditions.push("created_at >= ?".to_string());
                bindings.push(bson_to_iso(gte));
            }
            if let Ok(lt) = created_at_doc.get_datetime("$lt") {
                conditions.push("created_at < ?".to_string());
                bindings.push(bson_to_iso(lt));
            }
        }
        if let Ok(or_arr) = doc.get_array("$or") {
            let mut or_parts = Vec::new();
            for or_item in or_arr {
                if let Some(or_doc) = or_item.as_document() {
                    if let Ok(cn_doc) = or_doc.get_document("credit_note_number") {
                        if let Ok(regex) = cn_doc.get_str("$regex") {
                            let clean = regex.trim_start_matches('^').trim_end_matches('$');
                            or_parts.push("credit_note_number LIKE ?".to_string());
                            bindings.push(format!("%{clean}%"));
                        }
                    } else if let Ok(cn_exact) = or_doc.get_str("credit_note_number") {
                        or_parts.push("credit_note_number = ?".to_string());
                        bindings.push(cn_exact.to_string());
                    }

                    if let Ok(inv_doc) = or_doc.get_document("invoice_number") {
                        if let Ok(regex) = inv_doc.get_str("$regex") {
                            let clean = regex.trim_start_matches('^').trim_end_matches('$');
                            or_parts.push("invoice_number LIKE ?".to_string());
                            bindings.push(format!("%{clean}%"));
                        }
                    } else if let Ok(inv_exact) = or_doc.get_str("invoice_number") {
                        or_parts.push("invoice_number = ?".to_string());
                        bindings.push(inv_exact.to_string());
                    }

                    if let Ok(cname) = or_doc.get_document("customer_name_snapshot")
                        && let Ok(regex) = cname.get_str("$regex")
                    {
                        let clean = regex.trim_start_matches('^').trim_end_matches('$');
                        or_parts.push("customer_name_snapshot LIKE ?".to_string());
                        bindings.push(format!("%{clean}%"));
                    }
                }
            }
            if !or_parts.is_empty() {
                conditions.push(format!("({})", or_parts.join(" OR ")));
            }
        }
    }

    if let Ok(and_arr) = filter.get_array("$and") {
        for item in and_arr {
            if let Some(doc) = item.as_document() {
                process_clause(doc, &mut conditions, &mut bindings);
            }
        }
    } else {
        process_clause(filter, &mut conditions, &mut bindings);
    }

    SqlFilter {
        clause: conditions.join(" AND "),
        bindings,
    }
}
