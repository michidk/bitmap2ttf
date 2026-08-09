use write_fonts::OffsetMarker;
use write_fonts::tables::name::NameRecord;
use write_fonts::types::NameId;

pub(super) fn build_name_records(family_name: &str) -> Vec<NameRecord> {
    let postscript_name = sanitize_postscript_name(&format!("{family_name}-Regular"));
    let mut records = Vec::new();
    for &(name_id, value) in &[(1, family_name), (2, "Regular")] {
        records.push(NameRecord {
            platform_id: 1,
            encoding_id: 0,
            language_id: 0,
            name_id: NameId::from(name_id),
            string: OffsetMarker::new(value.to_string()),
        });
    }
    let windows_entries = [
        (1, family_name.to_string()),
        (2, "Regular".to_string()),
        (3, format!("{postscript_name};Version 1.0")),
        (4, format!("{family_name} Regular")),
        (5, "Version 1.0".to_string()),
        (6, postscript_name),
    ];
    for (name_id, value) in windows_entries {
        records.push(NameRecord {
            platform_id: 3,
            encoding_id: 1,
            language_id: 0x0409,
            name_id: NameId::from(name_id),
            string: OffsetMarker::new(value),
        });
    }
    records
}

fn sanitize_postscript_name(input: &str) -> String {
    let name: String = input
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
        .take(63)
        .collect();
    if name.is_empty() {
        "BitmapFont-Regular".to_string()
    } else {
        name
    }
}
