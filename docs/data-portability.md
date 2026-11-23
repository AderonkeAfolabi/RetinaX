# Data Portability & Export

## Supported Formats

- **CSV**: Comma-separated values for interoperability.
- **JSON**: Human-readable structured format.
- **CCD / CCDA**: Standard clinical document formats (XML-based).

## Bulk Export

Multiple patient records can be exported at once to any supported format using the `bulk_export` function.

## Import Validation

- Checks file existence.
- Validates file extension against format.
- Can be extended to validate schema.

## Usage Example

```rust
use contracts::vision_records::export::{PatientRecord, ExportFormat, bulk_export};

let patients = vec![
    PatientRecord { id: 1, name: "Alice".to_string(), date_of_birth: "1990-01-01".to_string(), conditions: vec!["Asthma".to_string()], medications: vec!["Inhaler".to_string()] }
];

bulk_export(&patients, &[ExportFormat::Csv, ExportFormat::Json, ExportFormat::Ccda], "./exports").unwrap();
```

## On-chain export: `export_patient_data` (issue #321)

The `vision_records` contract exposes `export_patient_data` for FHIR and CSV
export. The output is returned as Soroban `Bytes`.

```text
export_patient_data(caller, patient, format, offset, limit) -> Bytes
```

- `format`: `PortabilityFormat::FhirJson` (FHIR R4 `collection` Bundle) or
  `PortabilityFormat::Csv` (RFC 4180, header `section,record_type,date,field,value`).
- Who may call: the patient, an address with an active grant from the patient,
  or a system admin. Anyone else gets `AccessDenied`.
- Paging: `limit` is capped at 50 records per call. Start at `offset = 0` and
  advance by the page size until the record list is exhausted. Prescriptions
  and access grants are included on the first page only.
- Prescriptions and grants are not paged yet. If the patient has more than 50
  prescriptions, or more than 50 grantees, the first-page call fails with
  `InvalidInput`. Use `export_patient_data_page` below for lists of any size.

### Cursor-based bulk export: `export_patient_data_page` (issue #446)

```text
export_patient_data_page(caller, patient, format, cursor, limit) -> ExportPage
ExportPage { data: Bytes, next_cursor: Option<u32>, total: u32 }
```

- The export is one sequence: the patient's records, then their prescriptions,
  then their access grants. `cursor` is an index into that sequence.
- `limit` is capped at 50 items per page. Start at `cursor = 0` and follow
  `next_cursor` until it is `None`. Each page's `data` is a complete FHIR bundle
  or CSV document for the items it holds, so concatenate pages on the client
  side rather than parsing them as one document.
- `total` is the number of items, so a client can show progress. A cursor past
  `total` returns `InvalidInput`. A `limit` of 0 also returns `InvalidInput`.
- Authorisation is the same as `export_patient_data`.

### FHIR mapping

| Source              | Resource             | Notes                                                  |
|---------------------|----------------------|--------------------------------------------------------|
| Vision record       | `Observation`        | `effectiveDateTime` from the record creation time.     |
| Eye examination     | `Observation` parts  | Acuity, IOP as `valueQuantity` (`mm[Hg]`), slit lamp, visual field, retina, fundus. |
| Prescription        | `VisionPrescription` | Sphere, cylinder, axis, and add are numbers when valid JSON numbers; otherwise kept as notes. |
| Access grant        | `Consent`            | `permit` or `deny` from the grant level; period from grant start and expiry. |

### What is withheld

Internal identifiers are never written: record IDs, prescription IDs,
examination keys, the patient address, provider addresses, content hashes, and
image URLs. Grantee addresses are included on access grants because they are
the subject of the grant.

### Limits

- Records with examination data are exported in full. Records of other types
  export their type and date only, because their payload is not stored in a
  form this export reads.
- The export reads the grantee list, not an index of every grant. Grants that
  were revoked and removed from storage do not appear.
