use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use switchboard_guest_sdk as sdk;

pub(crate) enum Body {
    None,
    Json(&'static str),
}

pub(crate) struct Route {
    pub name: &'static str,
    pub description: &'static str,
    pub method: &'static str,
    pub path: &'static str,
    pub query: &'static [&'static str],
    pub body: Body,
    pub idempotent: bool,
}

const fn route(
    name: &'static str,
    description: &'static str,
    method: &'static str,
    path: &'static str,
    query: &'static [&'static str],
    body: Body,
    idempotent: bool,
) -> Route {
    Route {
        name,
        description,
        method,
        path,
        query,
        body,
        idempotent,
    }
}

const PAGE: &[&str] = &["limit", "offset"];
const NONE: &[&str] = &[];

pub(crate) const ROUTES: &[Route] = &[
    route("primer_list_lms_students", "List LMS students for reading plans. LMS student IDs differ from Tasks student IDs (primer_list_students); use these for studentId in reading plans and for primer_link_reading_tasks_student.", "GET", "/students", &["limit", "offset", "q", "sort", "dir", "filter"], Body::None, false),
    route("primer_get_lms_student", "Get one LMS student by UUID.", "GET", "/students/{id}", NONE, Body::None, false),
    route("primer_get_reading_capabilities", "Show which Primer reading library features are enabled: catalog writes, EPUB import, publication, print page mapping and question generation.", "GET", "/reading/capabilities", NONE, Body::None, false),
    route("primer_list_reading_works", "Search the Primer reading library. Start here for books: a work has editions (print or source), EPUB imports become revisions, and a published revision is what reading plans use. Returns work IDs for the other reading tools.", "GET", "/reading/works", &["q", "limit", "offset"], Body::None, false),
    route("primer_get_reading_work", "Get one reading library work with its version and active publication.", "GET", "/reading/works/{id}", NONE, Body::None, false),
    route("primer_create_reading_work", "Add a book to the Primer reading library.", "POST", "/reading/works", NONE, Body::Json("JSON {\"title\",\"author\",\"language\"}"), true),
    route("primer_update_reading_work", "Edit a reading work's title, author or language. Requires the current version from primer_get_reading_work.", "PUT", "/reading/works/{id}", NONE, Body::Json("JSON {\"version\",\"title\",\"author\",\"language\"}"), false),
    route("primer_list_reading_editions", "List a work's editions. Print editions carry page maps; source editions receive EPUB imports.", "GET", "/reading/works/{work_id}/editions", PAGE, Body::None, false),
    route("primer_get_reading_edition", "Get one reading edition.", "GET", "/reading/editions/{id}", NONE, Body::None, false),
    route("primer_create_reading_edition", "Add an edition to a reading work. Use kind \"source\" for an EPUB you will import and \"print\" for the physical book the student reads.", "POST", "/reading/works/{work_id}/editions", NONE, Body::Json("JSON {\"kind\":\"print|source\",\"title\",\"publisher\"?,\"publicationYear\"?,\"isbn\"?}"), true),
    route("primer_update_reading_edition", "Edit edition metadata. Requires the current version.", "PUT", "/reading/editions/{id}", NONE, Body::Json("JSON {\"version\",\"title\",\"publisher\"?,\"publicationYear\"?,\"isbn\"?}"), false),
    route("primer_list_reading_imports", "List recent EPUB imports and their processing state.", "GET", "/reading/imports", &["limit"], Body::None, false),
    route("primer_get_reading_import", "Get one EPUB import. When it finishes it names the draft revision to review and publish.", "GET", "/reading/imports/{id}", NONE, Body::None, false),
    route("primer_get_reading_import_draft", "Get the parsed table of contents and warnings for a finished EPUB import before publishing.", "GET", "/reading/imports/{id}/draft", NONE, Body::None, false),
    route("primer_cancel_reading_import", "Cancel a queued or running EPUB import.", "POST", "/reading/imports/{id}/cancel", NONE, Body::None, false),
    route("primer_retry_reading_import", "Retry a failed EPUB import.", "POST", "/reading/imports/{id}/retry", NONE, Body::None, false),
    route("primer_list_reading_revisions", "List a work's content revisions (drafts from imports and published versions).", "GET", "/reading/works/{work_id}/revisions", PAGE, Body::None, false),
    route("primer_get_reading_revision", "Get one content revision, including its version and bundle digest.", "GET", "/reading/works/{work_id}/revisions/{revision_id}", NONE, Body::None, false),
    route("primer_get_reading_revision_bundle", "Get a revision's content tree (chapters/sections with node and block IDs). Use node IDs for reading plan segments and page maps.", "GET", "/reading/works/{work_id}/revisions/{revision_id}/bundle", NONE, Body::None, false),
    route("primer_publish_reading_revision", "Publish a reviewed revision so reading plans can use it. Copy version and bundleSha256 from primer_get_reading_revision; acknowledge warnings and rights only after the parent has reviewed them.", "POST", "/reading/works/{work_id}/revisions/{revision_id}/publish", NONE, Body::Json("JSON {\"version\",\"bundleSha256\",\"acknowledgeWarnings\",\"acknowledgeRights\"}"), true),
    route("primer_retire_reading_revision", "Retire a published revision so new plans stop using it.", "POST", "/reading/works/{work_id}/revisions/{revision_id}/retire", NONE, Body::Json("JSON {\"version\"}"), false),
    route("primer_list_reading_maps", "List print page maps for an edition.", "GET", "/reading/editions/{edition_id}/maps", &["limit"], Body::None, false),
    route("primer_get_reading_map", "Get one print page map.", "GET", "/reading/maps/{id}", NONE, Body::None, false),
    route("primer_create_reading_map", "Record where chapters (nodeId) or specific paragraphs (nodeId plus blockId from the revision bundle) start in the student's printed book. Worksheets then show the pages to read and where to look for each question; pages between anchors are estimated. Publish it with primer_publish_reading_map.", "POST", "/reading/maps", NONE, Body::Json("JSON {\"workId\",\"editionId\",\"revisionId\",\"source\",\"assumptions\",\"anchors\":[{\"nodeId\",\"blockId\"?,\"pageLabel\",\"pageNumber\",\"sequence\":\"arabic|roman\",\"omitted\"}]}"), true),
    route("primer_publish_reading_map", "Publish a print page map.", "POST", "/reading/maps/{id}/publish", NONE, Body::Json("JSON {\"version\"}"), true),
    route("primer_delete_reading_map", "Delete a wrong print page map. Requires its current version; refused while a reading plan pins it. Worksheets then use the remaining published map for the edition. Confirm with the parent first.", "DELETE", "/reading/maps/{id}", &["version"], Body::None, false),
    route("primer_resolve_reading_map_page", "Find the content node at a printed page.", "GET", "/reading/maps/{id}/resolve", &["sequence", "page"], Body::None, false),
    route("primer_reverse_reading_map_node", "Find the printed page for a content node.", "GET", "/reading/maps/{id}/reverse", &["node_id"], Body::None, false),
    route("primer_list_reading_plans", "List reading plans, optionally for one LMS student. Start here for reading plans. Typical flow: create (quick) plan, schedule segments on education days, prepare a segment, generate and save questions, review, then publish the day's assignment to Tasks.", "GET", "/reading/plans", &["student_id", "limit", "offset"], Body::None, false),
    route("primer_get_reading_plan", "Get a reading plan with its version, segments, scheduled days and outcomes.", "GET", "/reading/plans/{id}", NONE, Body::None, false),
    route("primer_create_quick_reading_plan", "Create a reading plan that splits a published revision at chapter/section boundaries into segments of about targetMinutes and schedules them on weekdays from startDate. Easiest way to plan a book.", "POST", "/reading/plans/quick", NONE, Body::Json("JSON {\"studentId\",\"workId\",\"revisionId\",\"editionId\",\"mapId\"?,\"title\",\"startDate\":\"YYYY-MM-DD\",\"timezone\",\"targetMinutes\",\"wordsPerMinute\"}"), true),
    route("primer_create_reading_plan", "Create a reading plan with explicit segments and outcomes.", "POST", "/reading/plans", NONE, Body::Json("JSON PlanDraft {\"studentId\",\"workId\",\"revisionId\",\"editionId\",\"mapId\"?,\"title\",\"goals\",\"wordsPerMinute\",\"responseMinutes\",\"segments\":[{\"startNodeId\",\"endNodeId\",\"responseMinutes\"}],\"outcomes\":[{\"family\",\"description\",\"criteria\"}]}"), true),
    route("primer_update_reading_plan", "Replace a reading plan draft. Include the current version.", "PUT", "/reading/plans/{id}", NONE, Body::Json("JSON PlanDraft including \"version\""), false),
    route("primer_delete_reading_plan", "Delete a reading plan, canceling its Tasks assignments and removing related records. Requires the current plan version. Confirm with the parent first; this cannot be undone. If deletion is interrupted, retry to resume cleanup.", "DELETE", "/reading/plans/{id}", &["version"], Body::None, false),
    route("primer_resolve_reading_plan_range", "Check a structural range in the plan's revision and get its word count and estimate.", "POST", "/reading/plans/{id}/resolve", NONE, Body::Json("JSON {\"startNodeId\",\"endNodeId\",\"startBlockId\"?,\"endBlockId\"?}"), false),
    route("primer_schedule_reading_segment", "Put a plan segment on an education day for the student.", "PUT", "/reading/plans/{id}/segments/{segment_id}/day", NONE, Body::Json("JSON {\"version\",\"localDate\":\"YYYY-MM-DD\",\"timezone\"}"), false),
    route("primer_preview_reading_plan_adjustment", "Preview changing future segments, reading speed or response time.", "POST", "/reading/plans/{id}/adjustments/preview", NONE, Body::Json("JSON {\"version\",\"wordsPerMinute\",\"responseMinutes\",\"segments\"}"), false),
    route("primer_apply_reading_plan_adjustment", "Apply a previewed future-segment adjustment.", "POST", "/reading/plans/{id}/adjustments", NONE, Body::Json("JSON {\"version\",\"wordsPerMinute\",\"responseMinutes\",\"segments\"}"), true),
    route("primer_preview_reading_reinforcement", "Preview a reinforcement segment based on a parent review.", "POST", "/reading/plans/{id}/reinforcements/preview", NONE, Body::Json("JSON {\"version\",\"sourceReviewId\",\"sourceReviewVersion\",\"outcomeIds\",\"reason\",\"instruction\",\"segment\"}"), false),
    route("primer_propose_reading_reinforcement", "Add a previewed reinforcement segment to the plan.", "POST", "/reading/plans/{id}/reinforcements", NONE, Body::Json("JSON {\"version\",\"sourceReviewId\",\"sourceReviewVersion\",\"outcomeIds\",\"reason\",\"instruction\",\"segment\"}"), true),
    route("primer_list_reading_confirmations", "List ranges the parent confirmed the student has read, including retractions.", "GET", "/reading/plans/{id}/confirmations", NONE, Body::None, false),
    route("primer_confirm_reading_range", "Record that the parent confirmed the student read a range. Only do this when the parent says so.", "POST", "/reading/plans/{id}/confirmations", NONE, Body::Json("JSON {\"version\",\"startNodeId\",\"endNodeId\",\"source\":\"manual|source_review\"}"), false),
    route("primer_retract_reading_confirmation", "Retract a reading confirmation.", "POST", "/reading/plans/{id}/confirmations/{confirmation_id}/retract", NONE, Body::Json("JSON {\"version\"}"), false),
    route("primer_prepare_reading_segment", "Prepare a scheduled segment for question generation, choosing which confirmed prior ranges may be used as context.", "POST", "/reading/plans/{id}/prepare", NONE, Body::Json("JSON {\"segmentId\",\"priorConfirmationIds\":[]}"), false),
    route("primer_get_reading_preparation", "Get a segment preparation.", "GET", "/reading/plans/{id}/preparations/{preparation_id}", NONE, Body::None, false),
    route("primer_list_reading_question_jobs", "List question generation jobs for a plan.", "GET", "/reading/plans/{id}/question-candidates", NONE, Body::None, false),
    route("primer_generate_reading_questions", "Start AI question generation for one prepared segment. This may incur provider billing; set acknowledgePossibleBilling only with the parent's approval.", "POST", "/reading/plans/{id}/question-candidates", NONE, Body::Json("JSON {\"segmentId\",\"priorConfirmationIds\":[],\"profile\":{\"comprehensionCount\",\"reflectionCount\"},\"acknowledgePossibleBilling\"?}"), true),
    route("primer_generate_reading_question_batch", "Start AI question generation for several scheduled segments. This may incur provider billing.", "POST", "/reading/plans/{id}/question-candidates/batch", NONE, Body::Json("JSON {\"segmentIds\":[],\"profile\":{\"comprehensionCount\",\"reflectionCount\"}}"), true),
    route("primer_get_reading_question_job", "Get a question generation job and its candidate questions.", "GET", "/reading/plans/{id}/question-candidates/{job_id}", NONE, Body::None, false),
    route("primer_cancel_reading_question_job", "Cancel a question generation job.", "POST", "/reading/plans/{id}/question-candidates/{job_id}/cancel", NONE, Body::None, false),
    route("primer_retry_reading_question_job", "Retry a failed question generation job.", "POST", "/reading/plans/{id}/question-candidates/{job_id}/retry", NONE, Body::None, false),
    route("primer_get_reading_question_draft", "Get the saved question draft for a segment.", "GET", "/reading/plans/{id}/segments/{segment_id}/question-draft", NONE, Body::None, false),
    route("primer_save_reading_question_draft", "Save the parent's chosen and edited questions for a segment. Start from primer_get_reading_question_draft and keep its versions and scope.", "PUT", "/reading/plans/{id}/segments/{segment_id}/question-draft", NONE, Body::Json("JSON {\"version\",\"planVersion\",\"revisionId\",\"segmentScope\",\"items\"}"), false),
    route("primer_review_reading_assignment", "Show exactly what the student will receive for a segment's day, with the reviewedHash needed to publish.", "GET", "/reading/plans/{id}/segments/{segment_id}/assignment/review", NONE, Body::None, false),
    route("primer_publish_reading_assignment", "Publish a reviewed segment assignment to the student's Tasks day. Requires the reviewedHash from primer_review_reading_assignment and a linked Tasks student.", "POST", "/reading/plans/{id}/segments/{segment_id}/assignment", NONE, Body::Json("JSON {\"reviewedHash\"}"), true),
    route("primer_get_reading_assignment", "Get a published reading assignment and its Tasks delivery status.", "GET", "/reading/assignments/{id}", NONE, Body::None, false),
    route("primer_get_reading_academic_review", "Get the parent reviews synced back from Tasks for a reading assignment.", "GET", "/reading/assignments/{id}/academic-review", NONE, Body::None, false),
    route("primer_cancel_reading_assignment", "Cancel a published reading assignment.", "POST", "/reading/assignments/{id}/cancel", NONE, Body::None, false),
    route("primer_link_reading_tasks_student", "Start linking an LMS student to a Tasks student so reading assignments can be delivered. Returns a requestId and one-time secret: confirm them with primer_confirm_reading_tasks_link, then call primer_finalize_reading_tasks_student_link.", "PUT", "/reading/students/{student_id}/tasks-link", NONE, Body::Json("JSON {\"familyId\",\"tasksStudentId\",\"lmsParentId\"}"), false),
    route("primer_finalize_reading_tasks_student_link", "Finalize an LMS-to-Tasks student link.", "POST", "/reading/students/{student_id}/tasks-link/finalize", NONE, Body::Json("JSON {\"requestId\"}"), false),
    route("primer_revoke_reading_tasks_student_link", "Revoke an LMS-to-Tasks student link.", "POST", "/reading/students/{student_id}/tasks-link/revoke", NONE, Body::None, false),
    route("primer_list_learning_activities", "List workstation learning activities (terminal and typing lessons) loaded into the LMS. Filter with slug:<slug>. Use primer_list_learning_activity_revisions for the revision ID to assign.", "GET", "/learning-activities", &["q", "limit", "offset", "filter"], Body::None, false),
    route("primer_list_learning_activity_revisions", "List an activity's published revisions, newest last. Assign the revision ID with primer_create_learning_assignment.", "GET", "/learning-activities/{id}/revisions", NONE, Body::None, false),
    route("primer_plan_curriculum_import", "Validate a curriculum import bundle (standards, workstation activities and a course) and return a diff plus bundleDigest without changing anything that students see. Build the bundle from the Primer repo with: python3 content/technology/basic_linux/load.py --course-dir content/technology/<course> --bundle-out -. Follow with primer_apply_curriculum_import.", "POST", "/curriculum/import/plan", NONE, Body::Json("Import bundle JSON {\"schemaVersion\",\"sourceLabel\",\"version\",\"standards\":[],\"activities\":[],\"course\"}"), false),
    route("primer_apply_curriculum_import", "Apply a planned curriculum import. Publishes standards, activities and the course; never assigns or enrolls students. Pass the same bundle and the bundleDigest from primer_plan_curriculum_import.", "POST", "/curriculum/import/apply", NONE, Body::Json("JSON {\"bundle\":<bundle from primer_plan_curriculum_import>,\"bundleDigest\",\"sourceLabel\"}"), false),
    route("primer_list_student_learning_assignments", "List a student's workstation assignments and their state.", "GET", "/students/{id}/assignments", NONE, Body::None, false),
    route("primer_create_learning_assignment", "Assign a workstation activity revision to an LMS student. It appears in the student's work queue on the workstation after the next sync.", "POST", "/assignments", NONE, Body::Json("JSON {\"studentId\",\"activityRevisionId\",\"priority\"?,\"reason\"?}"), false),
    route("primer_cancel_learning_assignment", "Cancel a workstation assignment. Work already done is kept.", "POST", "/assignments/{id}/cancel", NONE, Body::None, false),
    route("primer_create_workstation_pairing_code", "Create a one-time code that pairs the student workstation to an LMS student. The student types it into the primer app on the workstation. Treat the code as a secret and give it only to the parent.", "POST", "/pairing-codes", NONE, Body::Json("JSON {\"studentId\"}"), false),
    route("primer_get_student_learning_overview", "Summarize a student's workstation learning: paired devices, open assignments, recent sessions and mastery.", "GET", "/students/{id}/learning-overview", NONE, Body::None, false),
    route("primer_get_student_activity_log", "Newest-first log of a student's workstation sessions, submitted short answers and each automated review outcome with the feedback the student saw.", "GET", "/students/{id}/activity-log", &["limit"], Body::None, false),
    route("primer_list_response_reviews", "List a student's short-answer responses by status (submitted, accepted, returned).", "GET", "/response-reviews", &["student_id", "status", "limit"], Body::None, false),
    route("primer_get_response_review", "Get one short-answer response with its prompt, rubric and review history.", "GET", "/response-reviews/{id}", NONE, Body::None, false),
    route("primer_decide_response_review", "Accept or return a saved short answer that automatic review could not decide. Only do this when the parent says so.", "POST", "/response-reviews/{id}/decision", NONE, Body::Json("JSON {\"decision\":\"accept|return\",\"reason\"?}"), false),
];

pub(crate) const IMPORT_TOOL: &str = "primer_import_reading_epub";
pub(crate) const IMPORT_DESCRIPTION: &str = "Import an EPUB into a source edition from an HTTPS download URL (up to 10 MiB). The plugin downloads it without Primer credentials, computes its SHA-256 and uploads it. Record where the book came from and its license; acknowledge rights only when the parent confirms the family may use it. Follow up with primer_get_reading_import.";
pub(crate) const MAX_EPUB_BYTES: usize = 10 * 1024 * 1024;

pub(crate) fn find(name: &str) -> Option<&'static Route> {
    ROUTES.iter().find(|route| route.name == name)
}

pub(crate) fn path_params(path: &str) -> Vec<&str> {
    path.split('/')
        .filter_map(|part| {
            part.strip_prefix('{')
                .and_then(|part| part.strip_suffix('}'))
        })
        .collect()
}

pub(crate) fn param_description(name: &str) -> &'static str {
    match name {
        "id" => "Record UUID",
        "work_id" => "Reading work UUID from primer_list_reading_works",
        "edition_id" => "Reading edition UUID from primer_list_reading_editions",
        "revision_id" => "Content revision UUID from primer_list_reading_revisions",
        "segment_id" => "Plan segment UUID from primer_get_reading_plan",
        "confirmation_id" => "Confirmation UUID from primer_list_reading_confirmations",
        "preparation_id" => "Preparation UUID returned by primer_prepare_reading_segment",
        "job_id" => "Question job UUID from primer_list_reading_question_jobs",
        "student_id" => "LMS student UUID from primer_list_lms_students",
        "limit" => "Maximum records to return",
        "status" => "Response status: submitted, accepted or returned",
        "offset" => "Records to skip",
        "q" => "Free-text search query",
        "sort" => "Field to sort by",
        "dir" => "Sort direction: asc or desc",
        "filter" => "Exact-match filter as column:value",
        "sequence" => "Page numbering sequence: arabic or roman",
        "page" => "Printed page label or number",
        "node_id" => "Content node ID from the revision bundle",
        "version" => "Current record version",
        _ => "",
    }
}

pub(crate) fn api_query_key(key: &str) -> &str {
    match key {
        "student_id" => "studentId",
        "node_id" => "nodeId",
        _ => key,
    }
}

pub(crate) fn idempotency_key(
    args: &HashMap<String, Value>,
    tool: &str,
    path: &str,
    body: &str,
) -> Result<String, sdk::ToolResult> {
    let supplied = sdk::arg_str(args, "idempotency_key");
    if !supplied.is_empty() {
        if supplied.len() > 128 || supplied.chars().any(|c| !c.is_ascii_graphic()) {
            return Err(sdk::err_result(
                "idempotency_key must be 1-128 visible ASCII characters",
            ));
        }
        return Ok(supplied);
    }
    let mut digest = Sha256::new();
    for part in [tool, path, body] {
        digest.update(part.as_bytes());
        digest.update([0]);
    }
    Ok(format!("sb-{}", &hex(&digest.finalize())[..48]))
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(crate) struct EpubImport {
    pub path: String,
    pub source_url: String,
    pub rights_source: String,
    pub rights_license: String,
    pub rights_acknowledged: bool,
}

pub(crate) fn epub_import(args: &HashMap<String, Value>) -> Result<EpubImport, String> {
    let work_id = sdk::arg_str(args, "work_id");
    let edition_id = sdk::arg_str(args, "edition_id");
    if work_id.is_empty() || edition_id.is_empty() {
        return Err("work_id and edition_id are required".into());
    }
    let source_url = sdk::arg_str(args, "source_url");
    if !source_url.starts_with("https://")
        || source_url.len() <= "https://".len()
        || source_url.chars().any(char::is_whitespace)
    {
        return Err("source_url must be an https:// download URL".into());
    }
    let rights_source = sdk::arg_str(args, "rights_source");
    let rights_license = sdk::arg_str(args, "rights_license");
    for (key, value) in [
        ("rights_source", &rights_source),
        ("rights_license", &rights_license),
    ] {
        if value.trim().is_empty() || value.len() > 500 || value.chars().any(char::is_control) {
            return Err(format!(
                "{key} is required (at most 500 characters, single line)"
            ));
        }
    }
    let rights_acknowledged = match sdk::arg_bool(args, "rights_acknowledged") {
        Some(value) => value,
        None => return Err("rights_acknowledged must be true or false".into()),
    };
    Ok(EpubImport {
        path: format!(
            "/reading/works/{}/editions/{}/imports",
            crate::percent_encode(&work_id),
            crate::percent_encode(&edition_id)
        ),
        source_url,
        rights_source,
        rights_license,
        rights_acknowledged,
    })
}

pub(crate) fn validate_epub(bytes: &[u8]) -> Result<(), String> {
    if bytes.is_empty() {
        return Err("EPUB download was empty".into());
    }
    if bytes.len() >= MAX_EPUB_BYTES {
        return Err("EPUB download reached the 10 MiB Switchboard transfer limit and may be truncated; upload it in the LMS instead".into());
    }
    if !bytes.starts_with(b"PK\x03\x04") {
        return Err("download is not an EPUB (ZIP) file".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn learning_administration_routes_cover_loading_and_review() {
        let import = find("primer_plan_curriculum_import").expect("plan");
        assert_eq!(
            (import.method, import.path),
            ("POST", "/curriculum/import/plan")
        );
        assert!(import.description.contains("--bundle-out"));
        let apply = find("primer_apply_curriculum_import").expect("apply");
        assert!(matches!(apply.body, Body::Json(text) if text.contains("bundleDigest")));
        let pair = find("primer_create_workstation_pairing_code").expect("pairing");
        assert_eq!((pair.method, pair.path), ("POST", "/pairing-codes"));
        let log = find("primer_get_student_activity_log").expect("log");
        assert_eq!(path_params(log.path), vec!["id"]);
        assert_eq!(log.query, &["limit"]);
        let reviews = find("primer_list_response_reviews").expect("reviews");
        assert_eq!(api_query_key(reviews.query[0]), "studentId");
        for name in [
            "primer_list_learning_activities",
            "primer_list_learning_activity_revisions",
            "primer_create_learning_assignment",
            "primer_list_student_learning_assignments",
            "primer_cancel_learning_assignment",
            "primer_get_student_learning_overview",
            "primer_get_response_review",
            "primer_decide_response_review",
        ] {
            assert!(find(name).is_some(), "{name}");
        }
    }

    fn args(pairs: &[(&str, Value)]) -> HashMap<String, Value> {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_string(), value.clone()))
            .collect()
    }

    #[test]
    fn routes_are_unique_documented_and_lms_scoped() {
        let mut names = ROUTES.iter().map(|route| route.name).collect::<Vec<_>>();
        names.push(IMPORT_TOOL);
        let count = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), count);
        for route in ROUTES {
            assert!(route.name.starts_with("primer_"), "{}", route.name);
            assert!(!route.description.is_empty(), "{}", route.name);
            assert!(
                [
                    "/reading/",
                    "/students",
                    "/learning-activities",
                    "/curriculum/import/",
                    "/assignments",
                    "/pairing-codes",
                    "/response-reviews"
                ]
                .iter()
                .any(|prefix| route.path.starts_with(prefix)),
                "{}",
                route.name
            );
            assert!(["GET", "POST", "PUT", "DELETE"].contains(&route.method));
            for name in path_params(route.path)
                .into_iter()
                .chain(route.query.iter().copied())
            {
                assert!(
                    !param_description(name).is_empty(),
                    "{}: {name}",
                    route.name
                );
            }
            if route.method == "GET" {
                assert!(matches!(route.body, Body::None), "{}", route.name);
                assert!(!route.idempotent, "{}", route.name);
            }
        }
        assert!(ROUTES
            .iter()
            .any(|route| route.name == "primer_list_reading_works"
                && route.description.contains("Start here")));
        assert!(ROUTES
            .iter()
            .any(|route| route.name == "primer_list_reading_plans"
                && route.description.contains("Start here")));
    }

    #[test]
    fn idempotency_keys_are_deterministic_bounded_and_overridable() {
        let empty = HashMap::new();
        let first = idempotency_key(&empty, "tool", "/reading/works", "{\"a\":1}")
            .ok()
            .unwrap();
        assert_eq!(
            first,
            idempotency_key(&empty, "tool", "/reading/works", "{\"a\":1}")
                .ok()
                .unwrap()
        );
        assert!(first.starts_with("sb-") && first.len() == 51);
        assert_ne!(
            first,
            idempotency_key(&empty, "tool", "/reading/works", "{\"a\":2}")
                .ok()
                .unwrap()
        );
        assert_ne!(
            first,
            idempotency_key(&empty, "other", "/reading/works", "{\"a\":1}")
                .ok()
                .unwrap()
        );
        let supplied = args(&[("idempotency_key", json!("retry-2"))]);
        assert_eq!(
            idempotency_key(&supplied, "tool", "/p", "").ok().unwrap(),
            "retry-2"
        );
        for invalid in ["has space", "tab\tkey", "\u{e9}"] {
            let bad = args(&[("idempotency_key", json!(invalid))]);
            assert!(
                idempotency_key(&bad, "tool", "/p", "").is_err(),
                "{invalid}"
            );
        }
        let long = args(&[("idempotency_key", json!("k".repeat(129)))]);
        assert!(idempotency_key(&long, "tool", "/p", "").is_err());
    }

    #[test]
    fn epub_import_requires_https_rights_and_ids() {
        let valid = args(&[
            ("work_id", json!("w/1")),
            ("edition_id", json!("e1")),
            ("source_url", json!("https://example.test/book.epub")),
            ("rights_source", json!("Standard Ebooks")),
            ("rights_license", json!("Public domain (US)")),
            ("rights_acknowledged", json!(true)),
        ]);
        let import = epub_import(&valid).unwrap();
        assert_eq!(import.path, "/reading/works/w%2F1/editions/e1/imports");
        assert!(import.rights_acknowledged);
        for (key, value) in [
            ("work_id", json!("")),
            ("source_url", json!("http://example.test/book.epub")),
            ("source_url", json!("https://")),
            ("source_url", json!("https://example.test/a b.epub")),
            ("rights_source", json!("")),
            ("rights_license", json!("line\nbreak")),
            ("rights_license", json!("x".repeat(501))),
        ] {
            let mut changed = valid.clone();
            changed.insert(key.into(), value);
            assert!(epub_import(&changed).is_err(), "{key}");
        }
        let mut missing = valid.clone();
        missing.remove("rights_acknowledged");
        assert!(epub_import(&missing).is_err());
        let mut declined = valid;
        declined.insert("rights_acknowledged".into(), json!("false"));
        assert!(!epub_import(&declined).unwrap().rights_acknowledged);
    }

    #[test]
    fn epub_payload_validation_rejects_truncation_and_non_zip() {
        assert!(validate_epub(b"PK\x03\x04rest").is_ok());
        assert!(validate_epub(b"").is_err());
        assert!(validate_epub(b"<html>").is_err());
        let mut truncated = b"PK\x03\x04".to_vec();
        truncated.resize(MAX_EPUB_BYTES, 0);
        assert!(validate_epub(&truncated).is_err());
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn query_keys_map_to_lms_names() {
        assert_eq!(api_query_key("student_id"), "studentId");
        assert_eq!(api_query_key("node_id"), "nodeId");
        assert_eq!(api_query_key("limit"), "limit");
    }
}
