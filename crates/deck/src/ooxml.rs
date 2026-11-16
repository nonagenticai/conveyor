//! The OOXML presentation package: parts, relationships, content types and the four slide
//! bodies.
//!
//! The package is deliberately minimal rather than a clone of a PowerPoint template. It has one
//! master, one blank layout, one theme and one slide part per deck slide; the slide carries its
//! own text boxes and, for the image layout, one `<p:pic>` whose `r:embed` points at a media
//! part. There is no second source of layout truth: the same fixed geometry is what makes the
//! package deterministic.
//!
//! Everything here is ordered by construction — parts are appended in a fixed sequence, shapes
//! are numbered with small integers, relationship ids are `rIdN` — so [`package`] has no
//! timestamp, random id or map iteration to vary.

use std::borrow::Cow;

use crate::{Deck, PreparedImage, Slide};

const XML_DECL: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>";
const NS_A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_P: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const NS_REL: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const NS_CT: &str = "http://schemas.openxmlformats.org/package/2006/content-types";

const REL_OFFICE_DOCUMENT: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
const REL_SLIDE_MASTER: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster";
const REL_SLIDE_LAYOUT: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout";
const REL_THEME: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme";
const REL_SLIDE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide";
const REL_NOTES_SLIDE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesSlide";
const REL_IMAGE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";

const CT_PRESENTATION: &str =
    "application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml";
const CT_SLIDE_MASTER: &str =
    "application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml";
const CT_SLIDE_LAYOUT: &str =
    "application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml";
const CT_THEME: &str = "application/vnd.openxmlformats-officedocument.theme+xml";
const CT_SLIDE: &str = "application/vnd.openxmlformats-officedocument.presentationml.slide+xml";
const CT_NOTES_SLIDE: &str =
    "application/vnd.openxmlformats-officedocument.presentationml.notesSlide+xml";

/// A 16:9 slide: 13.333 in × 7.5 in in EMU (914400 EMU per inch).
const SLIDE_WIDTH: i64 = 12192000;
const SLIDE_HEIGHT: i64 = 6858000;

/// One shape rectangle, in EMU.
#[derive(Clone, Copy)]
struct Rect {
    x: i64,
    y: i64,
    cx: i64,
    cy: i64,
}

const TITLE_RECT: Rect = Rect {
    x: 838200,
    y: 365125,
    cx: 10515600,
    cy: 1325563,
};
const SUBTITLE_RECT: Rect = Rect {
    x: 838200,
    y: 1825625,
    cx: 10515600,
    cy: 2387600,
};
const BODY_RECT: Rect = Rect {
    x: 838200,
    y: 1600200,
    cx: 10515600,
    cy: 4525963,
};
const CAPTION_RECT: Rect = Rect {
    x: 838200,
    y: 6032500,
    cx: 10515600,
    cy: 457200,
};

/// One XML part and its bytes. XML parts are owned; media parts borrow the image the caller
/// already loaded, so a deployment-sized deck does not hold every image in memory twice.
struct Part<'a> {
    name: String,
    data: Cow<'a, [u8]>,
}

impl<'a> Part<'a> {
    fn owned(name: impl Into<String>, xml: String) -> Self {
        Self {
            name: name.into(),
            data: Cow::Owned(xml.into_bytes()),
        }
    }

    fn media(name: String, bytes: &'a [u8]) -> Self {
        Self {
            name,
            data: Cow::Borrowed(bytes),
        }
    }
}

/// One slide's place in the package: its model, its image (when it is an image layout) and the
/// numbers the media and notes part names use.
struct SlidePlan<'a> {
    slide: &'a Slide,
    image: Option<&'a PreparedImage>,
    image_number: Option<usize>,
    notes_number: Option<usize>,
}

/// Write the whole package. The error is the name of a part that does not fit ZIP32.
pub(crate) fn package(deck: &Deck, images: &[Option<PreparedImage>]) -> Result<Vec<u8>, String> {
    let notes_count = deck
        .slides
        .iter()
        .filter(|slide| matches!(slide, Slide::Notes { .. }))
        .count();
    let mut parts: Vec<Part<'_>> = vec![
        Part::owned(
            "[Content_Types].xml",
            content_types(deck.slides.len(), notes_count),
        ),
        Part::owned("_rels/.rels", root_relationships()),
        Part::owned("ppt/presentation.xml", presentation(deck.slides.len())),
        Part::owned(
            "ppt/_rels/presentation.xml.rels",
            presentation_relationships(deck.slides.len()),
        ),
        Part::owned("ppt/slideMasters/slideMaster1.xml", slide_master()),
        Part::owned(
            "ppt/slideMasters/_rels/slideMaster1.xml.rels",
            master_relationships(),
        ),
        Part::owned("ppt/slideLayouts/slideLayout1.xml", slide_layout()),
        Part::owned(
            "ppt/slideLayouts/_rels/slideLayout1.xml.rels",
            layout_relationships(),
        ),
        Part::owned("ppt/theme/theme1.xml", theme()),
    ];

    let mut plans = Vec::with_capacity(deck.slides.len());
    let mut image_number = 0;
    let mut notes_number = 0;
    for (index, slide) in deck.slides.iter().enumerate() {
        let slide_image_number = match slide {
            Slide::Image { .. } => {
                image_number += 1;
                Some(image_number)
            }
            _ => None,
        };
        let slide_notes_number = match slide {
            Slide::Notes { .. } => {
                notes_number += 1;
                Some(notes_number)
            }
            _ => None,
        };
        plans.push(SlidePlan {
            slide,
            image: images.get(index).and_then(Option::as_ref),
            image_number: slide_image_number,
            notes_number: slide_notes_number,
        });
    }

    for (index, plan) in plans.iter().enumerate() {
        let number = index + 1;
        let (xml, relationships) = slide_part(plan);
        parts.push(Part::owned(format!("ppt/slides/slide{number}.xml"), xml));
        parts.push(Part::owned(
            format!("ppt/slides/_rels/slide{number}.xml.rels"),
            relationships,
        ));
        if let (Some(image), Some(image_number)) = (plan.image, plan.image_number) {
            parts.push(Part::media(
                format!("ppt/media/image{image_number}.{}", image.extension),
                &image.bytes,
            ));
        }
        if let Some(notes_number) = plan.notes_number {
            parts.push(Part::owned(
                format!("ppt/notesSlides/notesSlide{notes_number}.xml"),
                notes_slide(plan.slide),
            ));
            parts.push(Part::owned(
                format!("ppt/notesSlides/_rels/notesSlide{notes_number}.xml.rels"),
                notes_relationships(number),
            ));
        }
    }

    let entries: Vec<(&str, &[u8])> = parts
        .iter()
        .map(|part| (part.name.as_str(), part.data.as_ref()))
        .collect();
    crate::zip::archive(&entries)
}

/// `[Content_Types].xml`: a default for the two relationship/XML extensions and PNG/JPEG media,
/// plus one override per part that is read as OOXML.
fn content_types(slides: usize, notes: usize) -> String {
    let mut xml = String::from(XML_DECL);
    xml.push_str(&format!("<Types xmlns=\"{NS_CT}\">"));
    xml.push_str("<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>");
    xml.push_str("<Default Extension=\"xml\" ContentType=\"application/xml\"/>");
    xml.push_str("<Default Extension=\"png\" ContentType=\"image/png\"/>");
    xml.push_str("<Default Extension=\"jpeg\" ContentType=\"image/jpeg\"/>");
    xml.push_str(&content_type("/ppt/presentation.xml", CT_PRESENTATION));
    xml.push_str(&content_type(
        "/ppt/slideMasters/slideMaster1.xml",
        CT_SLIDE_MASTER,
    ));
    xml.push_str(&content_type(
        "/ppt/slideLayouts/slideLayout1.xml",
        CT_SLIDE_LAYOUT,
    ));
    xml.push_str(&content_type("/ppt/theme/theme1.xml", CT_THEME));
    for number in 1..=slides {
        xml.push_str(&content_type(
            &format!("/ppt/slides/slide{number}.xml"),
            CT_SLIDE,
        ));
    }
    for number in 1..=notes {
        xml.push_str(&content_type(
            &format!("/ppt/notesSlides/notesSlide{number}.xml"),
            CT_NOTES_SLIDE,
        ));
    }
    xml.push_str("</Types>");
    xml
}

fn content_type(part: &str, kind: &str) -> String {
    format!("<Override PartName=\"{part}\" ContentType=\"{kind}\"/>")
}

fn root_relationships() -> String {
    relationships_document(&relationship(
        "rId1",
        REL_OFFICE_DOCUMENT,
        "ppt/presentation.xml",
    ))
}

/// `ppt/presentation.xml`: the master, then the slides in deck order, then the two canvas
/// sizes. `rId1` is the master; slide `n` is `rId(n+1)`.
fn presentation(slides: usize) -> String {
    let mut ids = String::new();
    for number in 1..=slides {
        ids.push_str(&format!(
            "<p:sldId id=\"{}\" r:id=\"rId{}\"/>",
            255 + number,
            number + 1
        ));
    }
    format!(
        "{XML_DECL}<p:presentation xmlns:a=\"{NS_A}\" xmlns:r=\"{NS_R}\" xmlns:p=\"{NS_P}\">\
         <p:sldMasterIdLst><p:sldMasterId id=\"2147483648\" r:id=\"rId1\"/></p:sldMasterIdLst>\
         <p:sldIdLst>{ids}</p:sldIdLst>\
         <p:sldSz cx=\"{SLIDE_WIDTH}\" cy=\"{SLIDE_HEIGHT}\"/>\
         <p:notesSz cx=\"6858000\" cy=\"9144000\"/>\
         </p:presentation>"
    )
}

fn presentation_relationships(slides: usize) -> String {
    let mut body = relationship("rId1", REL_SLIDE_MASTER, "slideMasters/slideMaster1.xml");
    for number in 1..=slides {
        body.push_str(&relationship(
            &format!("rId{}", number + 1),
            REL_SLIDE,
            &format!("slides/slide{number}.xml"),
        ));
    }
    relationships_document(&body)
}

/// The master's shape tree is empty: every slide carries its own shapes, so the master exists to
/// give the package its theme and layout, not a second source of geometry.
fn slide_master() -> String {
    format!(
        "{XML_DECL}<p:sldMaster xmlns:a=\"{NS_A}\" xmlns:r=\"{NS_R}\" xmlns:p=\"{NS_P}\">\
         <p:cSld><p:spTree>{}</p:spTree></p:cSld>\
         <p:clrMap bg1=\"lt1\" tx1=\"dk1\" bg2=\"lt2\" tx2=\"dk2\" accent1=\"accent1\" \
         accent2=\"accent2\" accent3=\"accent3\" accent4=\"accent4\" accent5=\"accent5\" \
         accent6=\"accent6\" hlink=\"hlink\" folHlink=\"folHlink\"/>\
         <p:sldLayoutIdLst><p:sldLayoutId id=\"2147483649\" r:id=\"rId1\"/></p:sldLayoutIdLst>\
         </p:sldMaster>",
        group_props()
    )
}

fn master_relationships() -> String {
    let mut body = relationship("rId1", REL_SLIDE_LAYOUT, "../slideLayouts/slideLayout1.xml");
    body.push_str(&relationship("rId2", REL_THEME, "../theme/theme1.xml"));
    relationships_document(&body)
}

fn slide_layout() -> String {
    format!(
        "{XML_DECL}<p:sldLayout xmlns:a=\"{NS_A}\" xmlns:r=\"{NS_R}\" xmlns:p=\"{NS_P}\" \
         type=\"blank\" preserve=\"1\"><p:cSld name=\"Blank\"><p:spTree>{}</p:spTree></p:cSld>\
         <p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>",
        group_props()
    )
}

fn layout_relationships() -> String {
    relationships_document(&relationship(
        "rId1",
        REL_SLIDE_MASTER,
        "../slideMasters/slideMaster1.xml",
    ))
}

/// The theme the master references. The v1 template set does not theme individual runs, so this
/// is the smallest scheme that satisfies `a:themeElements`: a clrScheme, a fontScheme and a
/// fmtScheme with the three-entry style lists the schema requires.
fn theme() -> String {
    let mut xml = String::from(XML_DECL);
    xml.push_str(
        "<a:theme xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" \
         name=\"Conveyor\"><a:themeElements>\
         <a:clrScheme name=\"Conveyor\">\
         <a:dk1><a:sysClr val=\"windowText\" lastClr=\"000000\"/></a:dk1>\
         <a:lt1><a:sysClr val=\"window\" lastClr=\"FFFFFF\"/></a:lt1>\
         <a:dk2><a:srgbClr val=\"1F1F1F\"/></a:dk2>\
         <a:lt2><a:srgbClr val=\"F2F2F2\"/></a:lt2>\
         <a:accent1><a:srgbClr val=\"1A73E8\"/></a:accent1>\
         <a:accent2><a:srgbClr val=\"D93025\"/></a:accent2>\
         <a:accent3><a:srgbClr val=\"188038\"/></a:accent3>\
         <a:accent4><a:srgbClr val=\"F9AB00\"/></a:accent4>\
         <a:accent5><a:srgbClr val=\"9334E6\"/></a:accent5>\
         <a:accent6><a:srgbClr val=\"12B5CB\"/></a:accent6>\
         <a:hlink><a:srgbClr val=\"0563C1\"/></a:hlink>\
         <a:folHlink><a:srgbClr val=\"954F72\"/></a:folHlink>\
         </a:clrScheme>\
         <a:fontScheme name=\"Conveyor\">\
         <a:majorFont><a:latin typeface=\"Calibri\"/><a:ea typeface=\"\"/><a:cs typeface=\"\"/></a:majorFont>\
         <a:minorFont><a:latin typeface=\"Calibri\"/><a:ea typeface=\"\"/><a:cs typeface=\"\"/></a:minorFont>\
         </a:fontScheme>\
         <a:fmtScheme name=\"Conveyor\">\
         <a:fillStyleLst>\
         <a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill>\
         <a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill>\
         <a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill>\
         </a:fillStyleLst>\
         <a:lnStyleLst>\
         <a:ln w=\"9525\" cap=\"flat\" cmpd=\"sng\" algn=\"ctr\"><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill><a:prstDash val=\"solid\"/></a:ln>\
         <a:ln w=\"25400\" cap=\"flat\" cmpd=\"sng\" algn=\"ctr\"><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill><a:prstDash val=\"solid\"/></a:ln>\
         <a:ln w=\"38100\" cap=\"flat\" cmpd=\"sng\" algn=\"ctr\"><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill><a:prstDash val=\"solid\"/></a:ln>\
         </a:lnStyleLst>\
         <a:effectStyleLst>\
         <a:effectStyle><a:effectLst/></a:effectStyle>\
         <a:effectStyle><a:effectLst/></a:effectStyle>\
         <a:effectStyle><a:effectLst/></a:effectStyle>\
         </a:effectStyleLst>\
         <a:bgFillStyleLst>\
         <a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill>\
         <a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill>\
         <a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill>\
         </a:bgFillStyleLst>\
         </a:fmtScheme></a:themeElements>\
         <a:objectDefaults/><a:extraClrSchemeLst/></a:theme>",
    );
    xml
}

/// One slide part and its relationships. `rId1` is always the layout; an image or a notes slide
/// takes `rId2`, because no layout carries both.
fn slide_part(plan: &SlidePlan<'_>) -> (String, String) {
    let mut relationships =
        relationship("rId1", REL_SLIDE_LAYOUT, "../slideLayouts/slideLayout1.xml");
    let mut shapes = String::new();
    let mut next_shape = 2;

    match plan.slide {
        Slide::Title { title, subtitle } => {
            push_text(
                &mut shapes,
                &mut next_shape,
                "Title",
                TITLE_RECT,
                &[Paragraph::Plain(title)],
            );
            if let Some(subtitle) = subtitle {
                push_text(
                    &mut shapes,
                    &mut next_shape,
                    "Subtitle",
                    SUBTITLE_RECT,
                    &[Paragraph::Plain(subtitle)],
                );
            }
        }
        Slide::Bullets { title, bullets } => {
            if let Some(title) = title {
                push_text(
                    &mut shapes,
                    &mut next_shape,
                    "Title",
                    TITLE_RECT,
                    &[Paragraph::Plain(title)],
                );
            }
            let paragraphs: Vec<Paragraph<'_>> = bullets
                .iter()
                .map(|bullet| Paragraph::Bullet(bullet))
                .collect();
            push_text(&mut shapes, &mut next_shape, "Body", BODY_RECT, &paragraphs);
        }
        Slide::Image { title, caption, .. } => {
            if let Some(title) = title {
                push_text(
                    &mut shapes,
                    &mut next_shape,
                    "Title",
                    TITLE_RECT,
                    &[Paragraph::Plain(title)],
                );
            }
            let image = plan
                .image
                .expect("prepare loads an image for every image slide");
            let image_number = plan.image_number.expect("image slides are numbered");
            relationships.push_str(&relationship(
                "rId2",
                REL_IMAGE,
                &format!("../media/image{image_number}.{}", image.extension),
            ));
            shapes.push_str(&picture_shape(next_shape, "rId2"));
            next_shape += 1;
            if let Some(caption) = caption {
                push_text(
                    &mut shapes,
                    &mut next_shape,
                    "Caption",
                    CAPTION_RECT,
                    &[Paragraph::Plain(caption)],
                );
            }
        }
        Slide::Notes { title, .. } => {
            push_text(
                &mut shapes,
                &mut next_shape,
                "Title",
                TITLE_RECT,
                &[Paragraph::Plain(title)],
            );
            let notes_number = plan.notes_number.expect("notes slides are numbered");
            relationships.push_str(&relationship(
                "rId2",
                REL_NOTES_SLIDE,
                &format!("../notesSlides/notesSlide{notes_number}.xml"),
            ));
        }
    }

    (slide_xml(&shapes), relationships_document(&relationships))
}

fn slide_xml(shapes: &str) -> String {
    format!(
        "{XML_DECL}<p:sld xmlns:a=\"{NS_A}\" xmlns:r=\"{NS_R}\" xmlns:p=\"{NS_P}\">\
         <p:cSld><p:spTree>{}</p:spTree></p:cSld>\
         <p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>",
        group_props_and(shapes)
    )
}

fn group_props() -> String {
    group_props_and("")
}

/// The group shape every `p:spTree` starts with, plus the shapes that follow it.
fn group_props_and(shapes: &str) -> String {
    format!(
        "<p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>\
         <p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/>\
         <a:chOff x=\"0\" y=\"0\"/><a:chExt cx=\"0\" cy=\"0\"/></a:xfrm></p:grpSpPr>{shapes}"
    )
}

/// One paragraph of a text shape: a plain line, or a bulleted line.
enum Paragraph<'a> {
    Plain(&'a str),
    Bullet(&'a str),
}

fn push_text(
    shapes: &mut String,
    next_shape: &mut usize,
    label: &str,
    rect: Rect,
    paragraphs: &[Paragraph<'_>],
) {
    shapes.push_str(&text_shape(*next_shape, label, rect, paragraphs));
    *next_shape += 1;
}

fn text_shape(id: usize, label: &str, rect: Rect, paragraphs: &[Paragraph<'_>]) -> String {
    format!(
        "<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"{label} {id}\"/>\
         <p:cNvSpPr txBox=\"1\"/><p:nvPr/></p:nvSpPr>\
         <p:spPr><a:xfrm><a:off x=\"{x}\" y=\"{y}\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm>\
         <a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom><a:noFill/></p:spPr>\
         <p:txBody><a:bodyPr wrap=\"square\" rtlCol=\"0\"/><a:lstStyle/>{body}</p:txBody></p:sp>",
        x = rect.x,
        y = rect.y,
        cx = rect.cx,
        cy = rect.cy,
        body = paragraphs_xml(paragraphs)
    )
}

fn paragraphs_xml(paragraphs: &[Paragraph<'_>]) -> String {
    let mut body = String::new();
    for paragraph in paragraphs {
        match paragraph {
            Paragraph::Plain(text) => body.push_str(&format!(
                "<a:p><a:r><a:rPr lang=\"en-US\" dirty=\"0\"/><a:t xml:space=\"preserve\">{}</a:t></a:r></a:p>",
                escape(text)
            )),
            Paragraph::Bullet(text) => body.push_str(&format!(
                "<a:p><a:pPr marL=\"342900\" indent=\"-342900\"><a:buFont typeface=\"Arial\"/>\
                 <a:buChar char=\"&#8226;\"/></a:pPr><a:r><a:rPr lang=\"en-US\" dirty=\"0\"/>\
                 <a:t xml:space=\"preserve\">{}</a:t></a:r></a:p>",
                escape(text)
            )),
        }
    }
    if body.is_empty() {
        body.push_str("<a:p/>");
    }
    body
}

fn picture_shape(id: usize, relationship_id: &str) -> String {
    format!(
        "<p:pic><p:nvPicPr><p:cNvPr id=\"{id}\" name=\"Picture {id}\"/>\
         <p:cNvPicPr><a:picLocks noChangeAspect=\"1\"/></p:cNvPicPr><p:nvPr/></p:nvPicPr>\
         <p:blipFill><a:blip r:embed=\"{relationship_id}\"/><a:stretch><a:fillRect/></a:stretch></p:blipFill>\
         <p:spPr><a:xfrm><a:off x=\"1524000\" y=\"1200000\"/><a:ext cx=\"9144000\" cy=\"4800000\"/></a:xfrm>\
         <a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></p:spPr></p:pic>"
    )
}

/// `ppt/notesSlides/notesSlideN.xml`: one body placeholder carrying the notes paragraphs.
fn notes_slide(slide: &Slide) -> String {
    let Slide::Notes { notes, .. } = slide else {
        unreachable!("notes_slide is called only for the notes layout");
    };
    let paragraphs: Vec<Paragraph<'_>> = notes.iter().map(|note| Paragraph::Plain(note)).collect();
    format!(
        "{XML_DECL}<p:notes xmlns:a=\"{NS_A}\" xmlns:r=\"{NS_R}\" xmlns:p=\"{NS_P}\">\
         <p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>\
         <p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/>\
         <a:chOff x=\"0\" y=\"0\"/><a:chExt cx=\"0\" cy=\"0\"/></a:xfrm></p:grpSpPr>\
         <p:sp><p:nvSpPr><p:cNvPr id=\"2\" name=\"Notes Placeholder 1\"/>\
         <p:cNvSpPr><a:spLocks noGrp=\"1\"/></p:cNvSpPr>\
         <p:nvPr><p:ph type=\"body\" idx=\"1\"/></p:nvPr></p:nvSpPr>\
         <p:spPr><a:xfrm><a:off x=\"685800\" y=\"685800\"/><a:ext cx=\"5486400\" cy=\"7772400\"/></a:xfrm>\
         <a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></p:spPr>\
         <p:txBody><a:bodyPr/><a:lstStyle/>{body}</p:txBody></p:sp></p:spTree></p:cSld>\
         <p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:notes>",
        body = paragraphs_xml(&paragraphs)
    )
}

fn notes_relationships(slide_number: usize) -> String {
    relationships_document(&relationship(
        "rId1",
        REL_SLIDE,
        &format!("../slides/slide{slide_number}.xml"),
    ))
}

fn relationships_document(body: &str) -> String {
    format!("{XML_DECL}<Relationships xmlns=\"{NS_REL}\">{body}</Relationships>")
}

fn relationship(id: &str, kind: &str, target: &str) -> String {
    format!("<Relationship Id=\"{id}\" Type=\"{kind}\" Target=\"{target}\"/>")
}

/// Escape text for an XML text node. Quotes are escaped too: the same helper is safe if a caller
/// later interpolates a value into an attribute.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(character),
        }
    }
    out
}
