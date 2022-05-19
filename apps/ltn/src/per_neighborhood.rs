use geom::Distance;
use map_model::{IntersectionID, PathConstraints, RoadID};
use widgetry::mapspace::{ObjectID, World, WorldOutcome};
use widgetry::tools::open_browser;
use widgetry::{
    lctrl, EventCtx, Image, Key, Line, Panel, PanelBuilder, Text, TextExt, Widget,
    DEFAULT_CORNER_RADIUS,
};

use crate::shortcuts::Shortcuts;
use crate::{
    after_edit, colors, App, BrowseNeighborhoods, DiagonalFilter, Neighborhood, Transition,
};

// TODO This is only used for styling now
#[derive(PartialEq)]
pub enum Tab {
    Connectivity,
    Shortcuts,
}

impl Tab {
    pub fn panel_builder(
        self,
        ctx: &mut EventCtx,
        app: &App,
        top_panel: &Panel,
        per_tab_contents: Widget,
    ) -> PanelBuilder {
        let contents = Widget::col(vec![
            app.session.alt_proposals.to_widget(ctx, app),
            BrowseNeighborhoods::button(ctx, app),
            Line("Editing neighborhood")
                .small_heading()
                .into_widget(ctx),
            Widget::col(vec![
                Widget::row(vec![
                    Image::from_path("system/assets/tools/pencil.svg")
                        .into_widget(ctx)
                        .centered_vert(),
                    Text::from(Line(
                        "Click a road or intersection to add or remove a modal filter",
                    ))
                    .wrap_to_pct(ctx, 15)
                    .into_widget(ctx),
                ]),
                crate::components::FreehandFilters::button(ctx),
                Widget::row(vec![
                    format!(
                        "{} filters added",
                        app.session.modal_filters.roads.len()
                            + app.session.modal_filters.intersections.len()
                    )
                    .text_widget(ctx)
                    .centered_vert(),
                    ctx.style()
                        .btn_plain
                        .icon("system/assets/tools/undo.svg")
                        .disabled(app.session.modal_filters.previous_version.is_none())
                        .hotkey(lctrl(Key::Z))
                        .build_widget(ctx, "undo"),
                ]),
            ])
            .section(ctx),
            self.make_buttons(ctx, app),
            per_tab_contents,
            crate::route_planner::RoutePlanner::button(ctx),
        ]);
        crate::components::LeftPanel::builder(ctx, top_panel, contents)
    }

    fn make_buttons(self, ctx: &mut EventCtx, app: &App) -> Widget {
        let mut row = Vec::new();
        for (tab, label, key) in [
            (Tab::Connectivity, "Connectivity", Key::F1),
            (Tab::Shortcuts, "Shortcuts", Key::F2),
        ] {
            // TODO Match the TabController styling
            row.push(
                ctx.style()
                    .btn_tab
                    .text(label)
                    .corner_rounding(geom::CornerRadii {
                        top_left: DEFAULT_CORNER_RADIUS,
                        top_right: DEFAULT_CORNER_RADIUS,
                        bottom_left: 0.0,
                        bottom_right: 0.0,
                    })
                    .hotkey(key)
                    // We abuse "disabled" to denote "currently selected"
                    .disabled(self == tab)
                    .build_def(ctx),
            );
        }
        if app.session.consultation.is_none() {
            // TODO The 3rd doesn't really act like a tab
            row.push(
                ctx.style()
                    .btn_tab
                    .text("Adjust boundary")
                    .corner_rounding(geom::CornerRadii {
                        top_left: DEFAULT_CORNER_RADIUS,
                        top_right: DEFAULT_CORNER_RADIUS,
                        bottom_left: 0.0,
                        bottom_right: 0.0,
                    })
                    .hotkey(Key::B)
                    .build_def(ctx),
            );
        }

        Widget::row(row)
    }
}

pub fn handle_action(
    ctx: &mut EventCtx,
    app: &mut App,
    action: &str,
    neighborhood: &Neighborhood,
    panel: &Panel,
) -> Option<Transition> {
    let id = neighborhood.id;
    match action {
        "Browse neighborhoods" => {
            // Recalculate the state to redraw any changed filters
            Some(Transition::Replace(BrowseNeighborhoods::new_state(
                ctx, app,
            )))
        }
        "Adjust boundary" => Some(Transition::Replace(
            crate::select_boundary::SelectBoundary::new_state(ctx, app, id),
        )),
        "Connectivity" => Some(Transition::Replace(crate::connectivity::Viewer::new_state(
            ctx, app, id,
        ))),
        "Shortcuts" => Some(Transition::Replace(
            crate::shortcut_viewer::BrowseShortcuts::new_state(ctx, app, id, None),
        )),
        "Create filters along a shape" => Some(Transition::Push(
            crate::components::FreehandFilters::new_state(
                ctx,
                neighborhood,
                panel.center_of("Create filters along a shape"),
            ),
        )),
        "undo" => {
            let prev = app.session.modal_filters.previous_version.take().unwrap();
            app.session.modal_filters = prev;
            after_edit(ctx, app);
            // TODO Ideally, preserve panel state (checkboxes and dropdowns)
            Some(Transition::Recreate)
        }
        "Plan a route" => Some(Transition::Push(
            crate::route_planner::RoutePlanner::new_state(ctx, app),
        )),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FilterableObj {
    InteriorRoad(RoadID),
    InteriorIntersection(IntersectionID),
}
impl ObjectID for FilterableObj {}

/// Creates clickable objects for managing filters on roads and intersections. Everything is
/// invisible; the caller is responsible for drawing things.
pub fn make_world(
    ctx: &mut EventCtx,
    app: &App,
    neighborhood: &Neighborhood,
    shortcuts: &Shortcuts,
) -> World<FilterableObj> {
    let map = &app.map;
    let mut world = World::bounded(map.get_bounds());

    for r in &neighborhood.orig_perimeter.interior {
        let road = map.get_r(*r);
        world
            .add(FilterableObj::InteriorRoad(*r))
            .hitbox(road.get_thick_polygon())
            .drawn_in_master_batch()
            .hover_outline(colors::OUTLINE, Distance::meters(5.0))
            .tooltip(Text::from(format!(
                "{} shortcuts cross {}",
                shortcuts.count_per_road.get(*r),
                road.get_name(app.opts.language.as_ref()),
            )))
            .hotkey(lctrl(Key::D), "debug")
            .clickable()
            .build(ctx);
    }

    for i in &neighborhood.interior_intersections {
        world
            .add(FilterableObj::InteriorIntersection(*i))
            .hitbox(map.get_i(*i).polygon.clone())
            .drawn_in_master_batch()
            .hover_outline(colors::OUTLINE, Distance::meters(5.0))
            .tooltip(Text::from(format!(
                "{} shortcuts cross this intersection",
                shortcuts.count_per_intersection.get(*i)
            )))
            .clickable()
            .hotkey(lctrl(Key::D), "debug")
            .build(ctx);
    }

    world.initialize_hover(ctx);
    world
}

/// If true, the neighborhood has changed and the caller should recalculate stuff, including the
/// panel
pub fn handle_world_outcome(
    ctx: &mut EventCtx,
    app: &mut App,
    outcome: WorldOutcome<FilterableObj>,
) -> bool {
    let map = &app.map;
    match outcome {
        WorldOutcome::ClickedObject(FilterableObj::InteriorRoad(r)) => {
            let road = map.get_r(r);
            // Filtering a road that's already marked bike-only doesn't make sense. Likewise for
            // one-ways.
            if !PathConstraints::Car.can_use_road(road, map) || road.is_oneway() {
                return true;
            }

            app.session.modal_filters.before_edit();
            if app.session.modal_filters.roads.remove(&r).is_none() {
                // Place the filter on the part of the road that was clicked
                // These calls shouldn't fail -- since we clicked a road, the cursor must be in
                // map-space. And project_pt returns a point that's guaranteed to be on the
                // polyline.
                let cursor_pt = ctx.canvas.get_cursor_in_map_space().unwrap();
                let pt_on_line = road.center_pts.project_pt(cursor_pt);
                let (distance, _) = road.center_pts.dist_along_of_point(pt_on_line).unwrap();

                app.session.modal_filters.roads.insert(r, distance);
            }
            after_edit(ctx, app);
            true
        }
        WorldOutcome::ClickedObject(FilterableObj::InteriorIntersection(i)) => {
            app.session.modal_filters.before_edit();
            DiagonalFilter::cycle_through_alternatives(app, i);
            after_edit(ctx, app);
            true
        }
        WorldOutcome::Keypress("debug", FilterableObj::InteriorIntersection(i)) => {
            open_browser(app.map.get_i(i).orig_id.to_string());
            false
        }
        WorldOutcome::Keypress("debug", FilterableObj::InteriorRoad(r)) => {
            open_browser(app.map.get_r(r).orig_id.osm_way_id.to_string());
            false
        }
        _ => false,
    }
}
