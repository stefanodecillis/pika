use lsp_types::{
    ClientCapabilities, CodeActionClientCapabilities, CompletionClientCapabilities,
    GeneralClientCapabilities, GotoCapability, HoverClientCapabilities,
    PublishDiagnosticsClientCapabilities, ReferenceClientCapabilities,
    RenameClientCapabilities, SignatureHelpClientCapabilities,
    TextDocumentClientCapabilities, WorkspaceClientCapabilities,
    WorkspaceEditClientCapabilities,
    DocumentFormattingClientCapabilities,
    CodeActionLiteralSupport, CodeActionKindLiteralSupport,
    DidChangeConfigurationClientCapabilities,
};

/// Returns the `ClientCapabilities` that Pika advertises to language servers
/// during the `initialize` handshake.
pub fn client_capabilities() -> ClientCapabilities {
    ClientCapabilities {
        text_document: Some(TextDocumentClientCapabilities {
            completion: Some(CompletionClientCapabilities {
                ..Default::default()
            }),
            hover: Some(HoverClientCapabilities {
                ..Default::default()
            }),
            definition: Some(GotoCapability {
                ..Default::default()
            }),
            references: Some(ReferenceClientCapabilities {
                ..Default::default()
            }),
            rename: Some(RenameClientCapabilities {
                ..Default::default()
            }),
            code_action: Some(CodeActionClientCapabilities {
                code_action_literal_support: Some(CodeActionLiteralSupport {
                    code_action_kind: CodeActionKindLiteralSupport {
                        value_set: vec![
                            "quickfix".to_string(),
                            "refactor".to_string(),
                            "source".to_string(),
                        ],
                    },
                }),
                ..Default::default()
            }),
            formatting: Some(DocumentFormattingClientCapabilities {
                ..Default::default()
            }),
            publish_diagnostics: Some(PublishDiagnosticsClientCapabilities {
                ..Default::default()
            }),
            signature_help: Some(SignatureHelpClientCapabilities {
                ..Default::default()
            }),
            ..Default::default()
        }),
        workspace: Some(WorkspaceClientCapabilities {
            workspace_edit: Some(WorkspaceEditClientCapabilities {
                ..Default::default()
            }),
            did_change_configuration: Some(DidChangeConfigurationClientCapabilities {
                ..Default::default()
            }),
            ..Default::default()
        }),
        general: Some(GeneralClientCapabilities {
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Check whether a server advertises support for a given feature name.
///
/// Recognised feature names:
/// - `"completion"`, `"hover"`, `"definition"`, `"references"`, `"rename"`,
///   `"codeAction"`, `"formatting"`, `"signatureHelp"`, `"diagnostics"`
///
/// Returns `false` for unrecognised feature names.
pub fn supports_feature(
    server_caps: &lsp_types::ServerCapabilities,
    feature: &str,
) -> bool {
    match feature {
        "completion" => server_caps.completion_provider.is_some(),
        "hover" => server_caps.hover_provider.is_some(),
        "definition" => server_caps.definition_provider.is_some(),
        "references" => server_caps.references_provider.is_some(),
        "rename" => server_caps.rename_provider.is_some(),
        "codeAction" => server_caps.code_action_provider.is_some(),
        "formatting" => server_caps.document_formatting_provider.is_some(),
        "signatureHelp" => server_caps.signature_help_provider.is_some(),
        // publishDiagnostics is always a client-side capability; servers just
        // send the notification.  We report true unconditionally.
        "diagnostics" => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::ServerCapabilities;

    #[test]
    fn test_client_capabilities_has_text_document() {
        let caps = client_capabilities();
        assert!(caps.text_document.is_some());
    }

    #[test]
    fn test_client_capabilities_has_completion() {
        let caps = client_capabilities();
        let td = caps.text_document.as_ref().unwrap();
        assert!(td.completion.is_some());
    }

    #[test]
    fn test_client_capabilities_has_hover() {
        let caps = client_capabilities();
        let td = caps.text_document.as_ref().unwrap();
        assert!(td.hover.is_some());
    }

    #[test]
    fn test_client_capabilities_has_definition() {
        let caps = client_capabilities();
        let td = caps.text_document.as_ref().unwrap();
        assert!(td.definition.is_some());
    }

    #[test]
    fn test_client_capabilities_has_references() {
        let caps = client_capabilities();
        let td = caps.text_document.as_ref().unwrap();
        assert!(td.references.is_some());
    }

    #[test]
    fn test_client_capabilities_has_rename() {
        let caps = client_capabilities();
        let td = caps.text_document.as_ref().unwrap();
        assert!(td.rename.is_some());
    }

    #[test]
    fn test_client_capabilities_has_code_action() {
        let caps = client_capabilities();
        let td = caps.text_document.as_ref().unwrap();
        assert!(td.code_action.is_some());
    }

    #[test]
    fn test_client_capabilities_has_formatting() {
        let caps = client_capabilities();
        let td = caps.text_document.as_ref().unwrap();
        assert!(td.formatting.is_some());
    }

    #[test]
    fn test_client_capabilities_has_publish_diagnostics() {
        let caps = client_capabilities();
        let td = caps.text_document.as_ref().unwrap();
        assert!(td.publish_diagnostics.is_some());
    }

    #[test]
    fn test_client_capabilities_has_signature_help() {
        let caps = client_capabilities();
        let td = caps.text_document.as_ref().unwrap();
        assert!(td.signature_help.is_some());
    }

    #[test]
    fn test_client_capabilities_has_workspace() {
        let caps = client_capabilities();
        assert!(caps.workspace.is_some());
    }

    #[test]
    fn test_client_capabilities_workspace_edit() {
        let caps = client_capabilities();
        let ws = caps.workspace.as_ref().unwrap();
        assert!(ws.workspace_edit.is_some());
    }

    #[test]
    fn test_client_capabilities_did_change_configuration() {
        let caps = client_capabilities();
        let ws = caps.workspace.as_ref().unwrap();
        assert!(ws.did_change_configuration.is_some());
    }

    #[test]
    fn test_client_capabilities_code_action_kinds() {
        let caps = client_capabilities();
        let td = caps.text_document.as_ref().unwrap();
        let ca = td.code_action.as_ref().unwrap();
        let literal = ca.code_action_literal_support.as_ref().unwrap();
        let kinds = &literal.code_action_kind.value_set;
        assert!(kinds.contains(&"quickfix".to_string()));
        assert!(kinds.contains(&"refactor".to_string()));
        assert!(kinds.contains(&"source".to_string()));
    }

    #[test]
    fn test_supports_feature_completion() {
        let mut caps = ServerCapabilities::default();
        assert!(!supports_feature(&caps, "completion"));
        caps.completion_provider = Some(lsp_types::CompletionOptions::default());
        assert!(supports_feature(&caps, "completion"));
    }

    #[test]
    fn test_supports_feature_hover() {
        let mut caps = ServerCapabilities::default();
        assert!(!supports_feature(&caps, "hover"));
        caps.hover_provider = Some(lsp_types::HoverProviderCapability::Simple(true));
        assert!(supports_feature(&caps, "hover"));
    }

    #[test]
    fn test_supports_feature_definition() {
        let mut caps = ServerCapabilities::default();
        assert!(!supports_feature(&caps, "definition"));
        caps.definition_provider = Some(lsp_types::OneOf::Left(true));
        assert!(supports_feature(&caps, "definition"));
    }

    #[test]
    fn test_supports_feature_references() {
        let mut caps = ServerCapabilities::default();
        assert!(!supports_feature(&caps, "references"));
        caps.references_provider = Some(lsp_types::OneOf::Left(true));
        assert!(supports_feature(&caps, "references"));
    }

    #[test]
    fn test_supports_feature_rename() {
        let mut caps = ServerCapabilities::default();
        assert!(!supports_feature(&caps, "rename"));
        caps.rename_provider = Some(lsp_types::OneOf::Left(true));
        assert!(supports_feature(&caps, "rename"));
    }

    #[test]
    fn test_supports_feature_code_action() {
        let mut caps = ServerCapabilities::default();
        assert!(!supports_feature(&caps, "codeAction"));
        caps.code_action_provider =
            Some(lsp_types::CodeActionProviderCapability::Simple(true));
        assert!(supports_feature(&caps, "codeAction"));
    }

    #[test]
    fn test_supports_feature_formatting() {
        let mut caps = ServerCapabilities::default();
        assert!(!supports_feature(&caps, "formatting"));
        caps.document_formatting_provider = Some(lsp_types::OneOf::Left(true));
        assert!(supports_feature(&caps, "formatting"));
    }

    #[test]
    fn test_supports_feature_signature_help() {
        let mut caps = ServerCapabilities::default();
        assert!(!supports_feature(&caps, "signatureHelp"));
        caps.signature_help_provider = Some(lsp_types::SignatureHelpOptions::default());
        assert!(supports_feature(&caps, "signatureHelp"));
    }

    #[test]
    fn test_supports_feature_diagnostics_always_true() {
        let caps = ServerCapabilities::default();
        assert!(supports_feature(&caps, "diagnostics"));
    }

    #[test]
    fn test_supports_feature_unknown_returns_false() {
        let caps = ServerCapabilities::default();
        assert!(!supports_feature(&caps, "nonexistentFeature"));
        assert!(!supports_feature(&caps, ""));
    }

    #[test]
    fn test_client_capabilities_serialization() {
        let caps = client_capabilities();
        let json = serde_json::to_string(&caps).expect("serialize client capabilities");
        assert!(json.contains("completion"));
        assert!(json.contains("hover"));
    }
}
