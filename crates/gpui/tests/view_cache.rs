use gpui::{
    AnyView, AppContext as _, Context, Entity, IntoElement, ParentElement, Render,
    StyleRefinement, Styled, TestAppContext, Window, div,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

fn cached(view: impl Into<AnyView>) -> impl IntoElement {
    view.into().cached(StyleRefinement::default().size_full())
}

struct Text {
    lines: usize,
    renders: Arc<AtomicUsize>,
}

impl Render for Text {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        self.renders.fetch_add(1, Ordering::SeqCst);
        div()
            .flex()
            .flex_col()
            .children((0..self.lines).map(|i| div().child(format!("line {i}"))))
    }
}

struct Pair {
    busy: Entity<Text>,
    idle: Entity<Text>,
}

impl Render for Pair {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .child(cached(self.busy.clone()))
            .child(cached(self.idle.clone()))
    }
}

struct Root {
    other: Entity<Text>,
    pair: Entity<Pair>,
}

impl Render for Root {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .child(cached(self.other.clone()))
            .child(cached(self.pair.clone()))
    }
}

/// A notify re-renders the notified view's cached ancestors, but their other cached
/// descendants keep their last frame. That holds even when the ancestor itself was reused
/// in the frame before, which leaves the descendants' recorded ranges a frame behind.
#[gpui::test]
fn ancestor_of_a_notified_view_keeps_its_idle_children_cached(cx: &mut TestAppContext) {
    let busy_renders = Arc::new(AtomicUsize::new(0));
    let idle_renders = Arc::new(AtomicUsize::new(0));
    let mut handles = None;
    let window = {
        let busy_renders = busy_renders.clone();
        let idle_renders = idle_renders.clone();
        let handles = &mut handles;
        cx.add_window(move |_, cx| {
            let other = cx.new(|_| Text {
                lines: 20,
                renders: Arc::new(AtomicUsize::new(0)),
            });
            let busy = cx.new(|_| Text {
                lines: 1,
                renders: busy_renders,
            });
            let idle = cx.new(|_| Text {
                lines: 1,
                renders: idle_renders,
            });
            let pair = cx.new(|_| Pair {
                busy: busy.clone(),
                idle,
            });
            *handles = Some((other.clone(), busy));
            Root { other, pair }
        })
    };
    let (other, busy) = handles.expect("views created");
    let draw = |cx: &mut TestAppContext| {
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
    };

    draw(cx);
    busy_renders.store(0, Ordering::SeqCst);
    idle_renders.store(0, Ordering::SeqCst);

    // A notify flushes into a frame of its own. The extra draw after it is a frame in which
    // the pair is reused whole, after a sibling shrank, so the line layouts it carries move
    // to lower indices than its children recorded.
    cx.update(|cx| {
        other.update(cx, |text, cx| {
            text.lines = 1;
            cx.notify();
        })
    });
    draw(cx);
    assert_eq!(idle_renders.load(Ordering::SeqCst), 0);

    // Its children's ranges are now stale, so the idle one renders once more.
    cx.update(|cx| busy.update(cx, |_, cx| cx.notify()));
    assert_eq!(busy_renders.load(Ordering::SeqCst), 1);
    assert_eq!(idle_renders.load(Ordering::SeqCst), 1);

    // From then on it stays cached while its sibling keeps changing.
    for _ in 0..3 {
        cx.update(|cx| busy.update(cx, |_, cx| cx.notify()));
    }
    assert_eq!(busy_renders.load(Ordering::SeqCst), 4);
    assert_eq!(idle_renders.load(Ordering::SeqCst), 1);
}
