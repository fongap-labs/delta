//! Picture handling for a turn. When the chat model cannot read pictures, the vision model
//! describes each one before the turn runs, and the description is stored with the message. Each
//! outcome is recorded in the ledger without the description itself. A picture that cannot be
//! described leaves a note in its place, so the user and the model both see why it was not read.

use super::*;

impl RuntimeHost {
    /// The `vision_notes` for a new user message, or `None` when it needs none.
    pub(super) fn vision_notes(&self, attachments: &[Value]) -> Result<Option<Value>, String> {
        let pictures: Vec<(usize, &Value)> = attachments
            .iter()
            .enumerate()
            .filter(|(_, attachment)| crate::vision::is_picture(attachment))
            .collect();
        if pictures.is_empty() {
            return Ok(None);
        }
        let mut notes = Vec::with_capacity(pictures.len());
        for (index, picture) in pictures {
            let name = picture
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("image");
            let text = match &self.config.vision {
                crate::vision::VisionMode::Native => return Ok(None),
                crate::vision::VisionMode::Unavailable(reason) => {
                    format!("[image could not be read: {reason}]")
                }
                crate::vision::VisionMode::Bridge(bridge) => {
                    match self.describe_picture(bridge, picture) {
                        Ok(text) => {
                            self.ledger_transition(
                            "vision.described",
                            "vision",
                            json!({"name": name, "model": bridge.model, "outcome": "described", "chars": text.len()}),
                        )?;
                            text
                        }
                        Err(error) => {
                            self.ledger_transition(
                                "vision.described",
                                "vision",
                                json!({"name": name, "model": bridge.model, "outcome": "failed"}),
                            )?;
                            format!("[image could not be read: {error}]")
                        }
                    }
                }
            };
            notes.push(json!({"index": index, "text": format!("[image: {name}]\n{text}")}));
        }
        Ok(Some(Value::Array(notes)))
    }

    /// The `vision_notes` for the pictures a tool returned (a screenshot), or `None` when the chat
    /// model reads pictures itself. The notes are stored with the tool's result, so every later
    /// request sends the same words and the picture is described once.
    pub(super) fn tool_picture_notes(&self, images: &[Value]) -> Result<Option<Value>, String> {
        let pictures: Vec<Value> = images
            .iter()
            .filter_map(|image| self.screenshot_attachment(image))
            .collect();
        if pictures.is_empty() {
            return Ok(None);
        }
        self.vision_notes(&pictures)
    }

    /// Ask the vision model about one picture. The request passes the egress gate and uses this
    /// turn's proxy, like the summarizer's request does.
    fn describe_picture(
        &self,
        bridge: &crate::vision::VisionBridge,
        picture: &Value,
    ) -> Result<String, String> {
        let mut req = crate::vision::describe_request(bridge, picture)?;
        req.proxy = self.resolve_turn_proxy();
        let mut is_egress_confirmed = false;
        self.gate_model_request(&req, 0, &mut is_egress_confirmed)?;
        let response = crate::provider::complete(&req)?;
        crate::vision::response_text(&response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picture() -> Value {
        json!({
            "kind": "image",
            "name": "receipt.png",
            "mime": "image/png",
            "data_url": "data:image/png;base64,iVBORw0KGgo=",
        })
    }

    fn host(vision: crate::vision::VisionMode) -> RuntimeHost {
        let config = RuntimeConfig {
            vision,
            ..RuntimeConfig::default()
        };
        RuntimeHost::new("s1", config)
    }

    #[test]
    fn a_native_turn_adds_no_note() {
        let host = host(crate::vision::VisionMode::Native);
        assert!(host.vision_notes(&[picture()]).unwrap().is_none());
    }

    #[test]
    fn a_turn_without_pictures_adds_no_note_even_when_the_bridge_is_unavailable() {
        let host = host(crate::vision::VisionMode::Unavailable(
            "no vision model".into(),
        ));
        let text = json!({"kind": "text", "name": "a.txt", "text": "hi"});
        assert!(host.vision_notes(&[text]).unwrap().is_none());
    }

    #[test]
    fn an_unavailable_bridge_leaves_its_reason_in_place_of_the_picture() {
        let host = host(crate::vision::VisionMode::Unavailable(
            "the vision model could not be used: provider vision is not configured".into(),
        ));
        let notes = host.vision_notes(&[picture()]).unwrap().unwrap();
        let note = &notes[0];
        assert_eq!(note["index"], 0);
        let text = note["text"].as_str().unwrap();
        assert!(text.starts_with("[image: receipt.png]"), "{text}");
        assert!(text.contains("not configured"), "{text}");
    }

    #[test]
    fn a_failed_description_is_visible_and_does_not_drop_the_picture() {
        // Nothing listens on this port, so the vision request fails at once.
        let bridge = crate::vision::VisionBridge {
            model: "vision-model".into(),
            protocol: "openai_chat".into(),
            api_key: "key".into(),
            base_url: "http://127.0.0.1:9/v1".into(),
            max_image_bytes: 1 << 20,
            max_tokens: 256,
            timeout_secs: 5.0,
            prompt: crate::vision::DEFAULT_PROMPT.into(),
        };
        let host = host(crate::vision::VisionMode::Bridge(bridge));
        let notes = host.vision_notes(&[picture()]).unwrap().unwrap();
        let text = notes[0]["text"].as_str().unwrap();
        assert!(text.contains("[image could not be read:"), "{text}");
    }
}
