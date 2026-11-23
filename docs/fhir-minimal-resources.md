# Minimal FHIR R4 resources

Patient mapping accepts a pseudonymous FHIR id and an active flag. Use a random external identifier instead of a medical record number, wallet address, or identifier derived from clinical data. The mapping neither accepts nor stores names, identifiers, gender, or date of birth. Callers serialize the projection as a FHIR R4 Patient resource when exchanging it with an authorized recipient.

DiagnosticReport mapping carries a status, report coding, opaque Patient reference, effective timestamp, and up to 100 opaque Observation references. Keep narrative conclusions and report attachments in access-controlled off-chain storage.
