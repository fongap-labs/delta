//! PPTX: slides with a title and bullets or plain text, on a 16:9 master with two layouts.

use serde_json::Value;

use super::{array, esc, package, text_of, XML};

const MAX_SLIDES: usize = 300;
const MAX_BULLETS: usize = 100;

const NS: &str = "xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" \
    xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" \
    xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"";
const RELS_NS: &str = "xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

const GROUP_PROPS: &str = "<p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>\
    <p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/><a:chOff x=\"0\" y=\"0\"/><a:chExt cx=\"0\" cy=\"0\"/></a:xfrm></p:grpSpPr>";

fn placeholder(id: u32, name: &str, ph: &str, geometry: &str, body: &str) -> String {
    format!(
        "<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"{name}\"/><p:cNvSpPr><a:spLocks noGrp=\"1\"/></p:cNvSpPr>\
         <p:nvPr><p:ph {ph}/></p:nvPr></p:nvSpPr><p:spPr>{geometry}</p:spPr>{body}</p:sp>"
    )
}

fn xfrm(x: u32, y: u32, cx: u32, cy: u32) -> String {
    format!(
        "<a:xfrm><a:off x=\"{x}\" y=\"{y}\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom>"
    )
}

const TITLE_BOX: (u32, u32, u32, u32) = (838_200, 365_125, 10_515_600, 1_325_563);
const BODY_BOX: (u32, u32, u32, u32) = (838_200, 1_825_625, 10_515_600, 4_351_338);

fn prompt(text: &str) -> String {
    format!(
        "<p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang=\"en-US\"/><a:t>{text}</a:t></a:r></a:p></p:txBody>"
    )
}

fn master() -> String {
    let (tx, ty, tcx, tcy) = TITLE_BOX;
    let (bx, by, bcx, bcy) = BODY_BOX;
    let level = |n: u32, margin: u32, size: u32| {
        format!(
            "<a:lvl{n}pPr marL=\"{margin}\" indent=\"-285750\" algn=\"l\" defTabSz=\"914400\"><a:lnSpc><a:spcPct val=\"100000\"/></a:lnSpc>\
             <a:spcBef><a:spcPts val=\"600\"/></a:spcBef><a:buFont typeface=\"Arial\"/><a:buChar char=\"&#8226;\"/>\
             <a:defRPr sz=\"{size}\" kern=\"1200\"><a:solidFill><a:schemeClr val=\"tx1\"/></a:solidFill><a:latin typeface=\"+mn-lt\"/><a:ea typeface=\"+mn-ea\"/><a:cs typeface=\"+mn-cs\"/></a:defRPr></a:lvl{n}pPr>"
        )
    };
    format!(
        "{XML}<p:sldMaster {NS}><p:cSld><p:bg><p:bgRef idx=\"1001\"><a:schemeClr val=\"bg1\"/></p:bgRef></p:bg><p:spTree>{GROUP_PROPS}{}{}</p:spTree></p:cSld>\
         <p:clrMap bg1=\"lt1\" tx1=\"dk1\" bg2=\"lt2\" tx2=\"dk2\" accent1=\"accent1\" accent2=\"accent2\" accent3=\"accent3\" accent4=\"accent4\" accent5=\"accent5\" accent6=\"accent6\" hlink=\"hlink\" folHlink=\"folHlink\"/>\
         <p:sldLayoutIdLst><p:sldLayoutId id=\"2147483649\" r:id=\"rId1\"/><p:sldLayoutId id=\"2147483650\" r:id=\"rId2\"/></p:sldLayoutIdLst>\
         <p:txStyles><p:titleStyle><a:lvl1pPr algn=\"l\" defTabSz=\"914400\"><a:lnSpc><a:spcPct val=\"90000\"/></a:lnSpc><a:buNone/>\
         <a:defRPr sz=\"4000\" b=\"1\" kern=\"1200\"><a:solidFill><a:schemeClr val=\"tx2\"/></a:solidFill><a:latin typeface=\"+mj-lt\"/><a:ea typeface=\"+mj-ea\"/><a:cs typeface=\"+mj-cs\"/></a:defRPr></a:lvl1pPr></p:titleStyle>\
         <p:bodyStyle>{}{}{}</p:bodyStyle>\
         <p:otherStyle><a:defPPr><a:defRPr lang=\"en-US\"/></a:defPPr></p:otherStyle></p:txStyles></p:sldMaster>",
        placeholder(
            2,
            "Title Placeholder 1",
            "type=\"title\"",
            &xfrm(tx, ty, tcx, tcy),
            "<p:txBody><a:bodyPr vert=\"horz\" lIns=\"91440\" tIns=\"45720\" rIns=\"91440\" bIns=\"45720\" rtlCol=\"0\" anchor=\"ctr\"><a:normAutofit/></a:bodyPr><a:lstStyle/><a:p><a:r><a:rPr lang=\"en-US\"/><a:t>Click to edit Master title style</a:t></a:r></a:p></p:txBody>"
        ),
        placeholder(
            3,
            "Text Placeholder 2",
            "type=\"body\" idx=\"1\"",
            &xfrm(bx, by, bcx, bcy),
            "<p:txBody><a:bodyPr vert=\"horz\" lIns=\"91440\" tIns=\"45720\" rIns=\"91440\" bIns=\"45720\" rtlCol=\"0\"><a:normAutofit/></a:bodyPr><a:lstStyle/><a:p><a:pPr lvl=\"0\"/><a:r><a:rPr lang=\"en-US\"/><a:t>Click to edit Master text styles</a:t></a:r></a:p></p:txBody>"
        ),
        level(1, 285_750, 2800),
        level(2, 742_950, 2400),
        level(3, 1_200_150, 2000),
    )
}

fn layout(name: &str, kind: &str, with_body: bool) -> String {
    let body = if with_body {
        placeholder(
            3,
            "Content Placeholder 2",
            "idx=\"1\"",
            "",
            &prompt("Click to edit Master text styles"),
        )
    } else {
        String::new()
    };
    format!(
        "{XML}<p:sldLayout {NS} type=\"{kind}\" preserve=\"1\"><p:cSld name=\"{name}\"><p:spTree>{GROUP_PROPS}{}{body}</p:spTree></p:cSld>\
         <p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>",
        placeholder(2, "Title 1", "type=\"title\"", "", &prompt("Click to edit Master title style"))
    )
}

fn theme() -> String {
    let fills = "<a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill>".repeat(3);
    let lines = "<a:ln w=\"6350\" cap=\"flat\" cmpd=\"sng\" algn=\"ctr\"><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill><a:prstDash val=\"solid\"/></a:ln>".repeat(3);
    let effects = "<a:effectStyle><a:effectLst/></a:effectStyle>".repeat(3);
    let fonts =
        "<a:latin typeface=\"Calibri\"/><a:ea typeface=\"Microsoft YaHei\"/><a:cs typeface=\"\"/>";
    format!(
        "{XML}<a:theme xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" name=\"Delta\"><a:themeElements>\
         <a:clrScheme name=\"Delta\"><a:dk1><a:sysClr val=\"windowText\" lastClr=\"000000\"/></a:dk1><a:lt1><a:sysClr val=\"window\" lastClr=\"FFFFFF\"/></a:lt1>\
         <a:dk2><a:srgbClr val=\"1F3A4D\"/></a:dk2><a:lt2><a:srgbClr val=\"EEF2F5\"/></a:lt2>\
         <a:accent1><a:srgbClr val=\"2A7080\"/></a:accent1><a:accent2><a:srgbClr val=\"D9822B\"/></a:accent2><a:accent3><a:srgbClr val=\"5B8C5A\"/></a:accent3>\
         <a:accent4><a:srgbClr val=\"8E6BBF\"/></a:accent4><a:accent5><a:srgbClr val=\"C0504D\"/></a:accent5><a:accent6><a:srgbClr val=\"4F81BD\"/></a:accent6>\
         <a:hlink><a:srgbClr val=\"0563C1\"/></a:hlink><a:folHlink><a:srgbClr val=\"954F72\"/></a:folHlink></a:clrScheme>\
         <a:fontScheme name=\"Delta\"><a:majorFont>{fonts}</a:majorFont><a:minorFont>{fonts}</a:minorFont></a:fontScheme>\
         <a:fmtScheme name=\"Delta\"><a:fillStyleLst>{fills}</a:fillStyleLst><a:lnStyleLst>{lines}</a:lnStyleLst>\
         <a:effectStyleLst>{effects}</a:effectStyleLst><a:bgFillStyleLst>{fills}</a:bgFillStyleLst></a:fmtScheme>\
         </a:themeElements></a:theme>"
    )
}

/// One bullet: a string, or `{"text": ..., "level": 0-2}`.
fn bullet(item: &Value) -> Result<(String, u8), String> {
    match item {
        Value::Object(map) => {
            let text = text_of(map.get("text").unwrap_or(&Value::Null), "a bullet")?;
            let level = map.get("level").and_then(Value::as_u64).unwrap_or(0).min(2) as u8;
            Ok((text, level))
        }
        other => Ok((text_of(other, "a bullet")?, 0)),
    }
}

fn paragraph(text: &str, level: u8, bulleted: bool) -> String {
    let props = if bulleted {
        if level > 0 {
            format!("<a:pPr lvl=\"{level}\"/>")
        } else {
            String::new()
        }
    } else {
        "<a:pPr marL=\"0\" indent=\"0\"><a:buNone/></a:pPr>".to_owned()
    };
    format!("<a:p>{props}<a:r><a:rPr lang=\"zh-CN\" altLang=\"en-US\" dirty=\"0\"/><a:t>{}</a:t></a:r></a:p>", esc(text))
}

struct Slide {
    title: String,
    paragraphs: Vec<String>,
}

fn slide_xml(slide: &Slide) -> String {
    let title = placeholder(
        2,
        "Title 1",
        "type=\"title\"",
        "",
        &format!(
            "<p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang=\"zh-CN\" altLang=\"en-US\" dirty=\"0\"/><a:t>{}</a:t></a:r></a:p></p:txBody>",
            esc(&slide.title)
        ),
    );
    let body = if slide.paragraphs.is_empty() {
        String::new()
    } else {
        placeholder(
            3,
            "Content Placeholder 2",
            "idx=\"1\"",
            "",
            &format!(
                "<p:txBody><a:bodyPr><a:normAutofit/></a:bodyPr><a:lstStyle/>{}</p:txBody>",
                slide.paragraphs.concat()
            ),
        )
    };
    format!("{XML}<p:sld {NS}><p:cSld><p:spTree>{GROUP_PROPS}{title}{body}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>")
}

pub fn build(arguments: &Value) -> Result<Vec<u8>, String> {
    let raw_slides = array(arguments, "slides")?;
    if raw_slides.is_empty() {
        return Err("slides must not be empty".to_owned());
    }
    if raw_slides.len() > MAX_SLIDES {
        return Err(format!(
            "a presentation may have at most {MAX_SLIDES} slides"
        ));
    }
    let mut slides = Vec::new();
    for (index, raw) in raw_slides.iter().enumerate() {
        let at = |what: &str| format!("slide {}: {what}", index + 1);
        let title = match raw.get("title") {
            None | Some(Value::Null) => String::new(),
            Some(value) => text_of(value, "the title").map_err(|e| at(&e))?,
        };
        let mut paragraphs = Vec::new();
        if let Some(text) = raw.get("text").filter(|v| !v.is_null()) {
            for line in text_of(text, "text")
                .map_err(|e| at(&e))?
                .lines()
                .filter(|l| !l.trim().is_empty())
            {
                paragraphs.push(paragraph(line.trim(), 0, false));
            }
        }
        if let Some(bullets) = raw.get("bullets").filter(|v| !v.is_null()) {
            let bullets = bullets
                .as_array()
                .ok_or_else(|| at("bullets must be a list"))?;
            if bullets.len() > MAX_BULLETS {
                return Err(at(&format!("at most {MAX_BULLETS} bullets")));
            }
            for item in bullets {
                let (text, level) = bullet(item).map_err(|e| at(&e))?;
                paragraphs.push(paragraph(&text, level, true));
            }
        }
        slides.push(Slide { title, paragraphs });
    }

    let mut content_types = format!(
        "{XML}<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
         <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
         <Default Extension=\"xml\" ContentType=\"application/xml\"/>\
         <Override PartName=\"/ppt/presentation.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml\"/>\
         <Override PartName=\"/ppt/slideMasters/slideMaster1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml\"/>\
         <Override PartName=\"/ppt/slideLayouts/slideLayout1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml\"/>\
         <Override PartName=\"/ppt/slideLayouts/slideLayout2.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml\"/>\
         <Override PartName=\"/ppt/theme/theme1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.theme+xml\"/>"
    );
    let mut presentation_rels = format!(
        "{XML}<Relationships {RELS_NS}><Relationship Id=\"rId1\" Type=\"{REL}/slideMaster\" Target=\"slideMasters/slideMaster1.xml\"/>\
         <Relationship Id=\"rId2\" Type=\"{REL}/theme\" Target=\"theme/theme1.xml\"/>"
    );
    let mut slide_ids = String::new();
    let mut parts: Vec<(String, String)> = Vec::new();
    for (index, slide) in slides.iter().enumerate() {
        let n = index + 1;
        content_types.push_str(&format!(
            "<Override PartName=\"/ppt/slides/slide{n}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slide+xml\"/>"
        ));
        presentation_rels.push_str(&format!(
            "<Relationship Id=\"rId{}\" Type=\"{REL}/slide\" Target=\"slides/slide{n}.xml\"/>",
            n + 2
        ));
        slide_ids.push_str(&format!(
            "<p:sldId id=\"{}\" r:id=\"rId{}\"/>",
            255 + n,
            n + 2
        ));
        let layout_number = if slide.paragraphs.is_empty() { 2 } else { 1 };
        parts.push((format!("ppt/slides/slide{n}.xml"), slide_xml(slide)));
        parts.push((
            format!("ppt/slides/_rels/slide{n}.xml.rels"),
            format!(
                "{XML}<Relationships {RELS_NS}><Relationship Id=\"rId1\" Type=\"{REL}/slideLayout\" Target=\"../slideLayouts/slideLayout{layout_number}.xml\"/></Relationships>"
            ),
        ));
    }
    content_types.push_str("</Types>");
    presentation_rels.push_str("</Relationships>");

    let mut all = vec![
        ("[Content_Types].xml".to_owned(), content_types),
        (
            "_rels/.rels".to_owned(),
            format!(
                "{XML}<Relationships {RELS_NS}><Relationship Id=\"rId1\" Type=\"{REL}/officeDocument\" Target=\"ppt/presentation.xml\"/></Relationships>"
            ),
        ),
        (
            "ppt/presentation.xml".to_owned(),
            format!(
                "{XML}<p:presentation {NS} saveSubsetFonts=\"1\"><p:sldMasterIdLst><p:sldMasterId id=\"2147483648\" r:id=\"rId1\"/></p:sldMasterIdLst>\
                 <p:sldIdLst>{slide_ids}</p:sldIdLst><p:sldSz cx=\"12192000\" cy=\"6858000\"/><p:notesSz cx=\"6858000\" cy=\"9144000\"/></p:presentation>"
            ),
        ),
        ("ppt/_rels/presentation.xml.rels".to_owned(), presentation_rels),
        ("ppt/slideMasters/slideMaster1.xml".to_owned(), master()),
        (
            "ppt/slideMasters/_rels/slideMaster1.xml.rels".to_owned(),
            format!(
                "{XML}<Relationships {RELS_NS}><Relationship Id=\"rId1\" Type=\"{REL}/slideLayout\" Target=\"../slideLayouts/slideLayout1.xml\"/>\
                 <Relationship Id=\"rId2\" Type=\"{REL}/slideLayout\" Target=\"../slideLayouts/slideLayout2.xml\"/>\
                 <Relationship Id=\"rId3\" Type=\"{REL}/theme\" Target=\"../theme/theme1.xml\"/></Relationships>"
            ),
        ),
        ("ppt/slideLayouts/slideLayout1.xml".to_owned(), layout("Title and Content", "obj", true)),
        ("ppt/slideLayouts/slideLayout2.xml".to_owned(), layout("Title Only", "titleOnly", false)),
    ];
    for n in [1, 2] {
        all.push((
            format!("ppt/slideLayouts/_rels/slideLayout{n}.xml.rels"),
            format!(
                "{XML}<Relationships {RELS_NS}><Relationship Id=\"rId1\" Type=\"{REL}/slideMaster\" Target=\"../slideMasters/slideMaster1.xml\"/></Relationships>"
            ),
        ));
    }
    all.push(("ppt/theme/theme1.xml".to_owned(), theme()));
    all.extend(parts);
    package(all)
}
