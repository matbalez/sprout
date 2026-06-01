use nostr::Tag;

use super::tag;

pub(super) fn annotation_tags(
    annotation_tags: &[Vec<String>],
    tags: &mut Vec<Tag>,
) -> Result<(), String> {
    for annotation in annotation_tags {
        match annotation.as_slice() {
            [name, feature, version]
                if name == "sprout" && feature == "kudos" && version == "v1" =>
            {
                tags.push(tag(vec!["sprout", "kudos", "v1"])?);
            }
            _ => {
                return Err(format!("unsupported annotation tag: {annotation:?}"));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{EventBuilder, Keys, Kind};

    #[test]
    fn accepts_only_sprout_kudos_annotation() {
        let mut tags = Vec::new();

        annotation_tags(
            &[vec!["sprout".into(), "kudos".into(), "v1".into()]],
            &mut tags,
        )
        .unwrap();

        let event = EventBuilder::new(Kind::Custom(9), "great work")
            .tags(tags)
            .sign_with_keys(&Keys::generate())
            .unwrap();
        let tags: Vec<Vec<String>> = event.tags.iter().map(|t| t.as_slice().to_vec()).collect();

        assert!(tags.contains(&vec![
            "sprout".to_string(),
            "kudos".to_string(),
            "v1".to_string(),
        ]));

        let mut tags = Vec::new();
        let err = annotation_tags(&[vec!["e".into(), "forged".into()]], &mut tags).unwrap_err();
        assert!(err.contains("unsupported annotation tag"));
    }
}
