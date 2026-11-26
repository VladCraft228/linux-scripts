/* SPDX-License-Identifier: MIT */
#include <gio/gio.h>
#include <gtk/gtk.h>
#include <libxfce4panel/libxfce4panel.h>
#include <pango/pangocairo.h>
#include <string.h>

typedef struct {
    GtkWidget *box;
    GtkWidget *image;
    GtkWidget *label;
    GtkWidget *art;
    GdkRGBA accent;
    gint percentage_style;
    gint charging_bolt;
    gint battery_level;
    gint icon_size;
    gboolean charging;
    GDBusProxy *device;
    GFileMonitor *settings_monitor;
    guint visible_owner;
    gchar *settings_dir;
} BatteryIndicator;

enum {
    PERCENT_TOOLTIP,
    PERCENT_INSIDE,
    PERCENT_BESIDE
};

enum {
    BOLT_ACCENT,
    BOLT_WHITE,
    BOLT_HIDDEN
};

static void resize_art(BatteryIndicator *indicator)
{
    gint size = MAX(12, indicator->icon_size);
    gint width = size * 5 / 4;
    if (indicator->percentage_style == PERCENT_INSIDE)
        width = size * 2;
    else if (indicator->percentage_style == PERCENT_BESIDE)
        width = size * 3 / 4;
    gtk_widget_set_size_request(indicator->art, width, size);
}

static gboolean draw_battery(GtkWidget *widget, cairo_t *cr, BatteryIndicator *indicator)
{
    gdouble width = gtk_widget_get_allocated_width(widget);
    gdouble height = gtk_widget_get_allocated_height(widget);
    if (width < 8.0 || height < 8.0 || indicator->battery_level < 0)
        return FALSE;
    gboolean inside = indicator->percentage_style == PERCENT_INSIDE;
    gboolean beside = indicator->percentage_style == PERCENT_BESIDE;
    gboolean bolt = indicator->charging && indicator->charging_bolt != BOLT_HIDDEN;
    gdouble battery_width = beside ? height : width;
    gdouble battery_height = beside ? width : height;
    gdouble body_width = battery_width - 4.0;
    gdouble body_height = battery_height * 0.70;
    gdouble body_y = (battery_height - body_height) / 2.0;

    cairo_save(cr);
    if (beside) {
        cairo_translate(cr, 0.0, height);
        cairo_rotate(cr, -G_PI_2);
    }

    cairo_set_source_rgba(cr, 0.08, 0.11, 0.15, 0.92);
    cairo_rectangle(cr, 1.0, body_y, body_width, body_height);
    cairo_fill_preserve(cr);
    gdk_cairo_set_source_rgba(cr, &indicator->accent);
    cairo_set_line_width(cr, 1.5);
    cairo_stroke(cr);
    cairo_rectangle(cr, body_width + 1.75, battery_height * 0.36, 2.0, battery_height * 0.28);
    cairo_fill(cr);

    GdkRGBA fill = indicator->accent;
    fill.alpha = inside || bolt ? 0.42 : 0.90;
    gdk_cairo_set_source_rgba(cr, &fill);
    cairo_rectangle(cr, 3.0, body_y + 2.0,
                    (body_width - 4.0) * indicator->battery_level / 100.0, body_height - 4.0);
    cairo_fill(cr);
    cairo_restore(cr);

    if (inside) {
        g_autofree gchar *text = g_strdup_printf("%d%%", indicator->battery_level);
        PangoLayout *layout = gtk_widget_create_pango_layout(widget, text);
        PangoFontDescription *font = pango_font_description_new();
        pango_font_description_set_weight(font, PANGO_WEIGHT_BOLD);
        gdouble font_size = height * 0.54 * PANGO_SCALE;
        gdouble text_space = bolt ? body_width * 0.82 - height * 0.18 - 3.0 : body_width - 3.0;
        pango_font_description_set_absolute_size(font, font_size);
        pango_layout_set_font_description(layout, font);
        gint text_width, text_height;
        pango_layout_get_pixel_size(layout, &text_width, &text_height);
        if (text_width > text_space && text_space > 0.0) {
            pango_font_description_set_absolute_size(font, font_size * text_space / text_width);
            pango_layout_set_font_description(layout, font);
            pango_layout_get_pixel_size(layout, &text_width, &text_height);
        }
        pango_font_description_free(font);
        cairo_move_to(cr, MAX(2.0, 2.0 + (text_space - text_width) / 2.0), (height - text_height) / 2.0);
        cairo_set_source_rgb(cr, 1.0, 1.0, 1.0);
        pango_cairo_show_layout(cr, layout);
        g_object_unref(layout);
    }

    if (bolt) {
        gdouble x = beside ? width * 0.5 : body_width * (inside ? 0.82 : 0.52);
        gdouble y = beside ? height * 0.55 : height * 0.5;
        gdouble w = MIN(height * 0.18, width * 0.24);
        gdouble h = height * 0.29;
        cairo_move_to(cr, x + w * 0.3, y - h);
        cairo_line_to(cr, x - w, y + h * 0.10);
        cairo_line_to(cr, x - w * 0.12, y + h * 0.10);
        cairo_line_to(cr, x - w * 0.32, y + h);
        cairo_line_to(cr, x + w, y - h * 0.12);
        cairo_line_to(cr, x + w * 0.10, y - h * 0.12);
        cairo_close_path(cr);
        if (indicator->charging_bolt == BOLT_WHITE)
            cairo_set_source_rgb(cr, 1.0, 1.0, 1.0);
        else
            gdk_cairo_set_source_rgba(cr, &indicator->accent);
        cairo_fill(cr);
    }
    return FALSE;
}

static void update_battery(GtkWidget *button)
{
    BatteryIndicator *indicator = g_object_get_data(G_OBJECT(button), "vatra-indicator");
    GDBusProxy *device = indicator->device;
    g_autofree gchar *owner = device ? g_dbus_proxy_get_name_owner(device) : NULL;
    g_autoptr(GVariant) present = owner ? g_dbus_proxy_get_cached_property(device, "IsPresent") : NULL;
    g_autoptr(GVariant) percentage = owner ? g_dbus_proxy_get_cached_property(device, "Percentage") : NULL;
    g_autoptr(GVariant) icon = owner ? g_dbus_proxy_get_cached_property(device, "IconName") : NULL;
    g_autoptr(GVariant) state = owner ? g_dbus_proxy_get_cached_property(device, "State") : NULL;
    indicator->charging = state && g_variant_is_of_type(state, G_VARIANT_TYPE_UINT32)
        && g_variant_get_uint32(state) == 1; /* UPower Charging */
    indicator->battery_level = -1;

    const gchar *icon_name = "battery-missing-symbolic";
    const gchar *label = "--";
    const gchar *tooltip = "Vatra — Battery status unavailable";
    g_autofree gchar *charge = NULL;
    g_autofree gchar *charge_tooltip = NULL;
    if (present && g_variant_is_of_type(present, G_VARIANT_TYPE_BOOLEAN)) {
        if (g_variant_get_boolean(present)) {
            const gchar *name = icon && g_variant_is_of_type(icon, G_VARIANT_TYPE_STRING)
                ? g_variant_get_string(icon, NULL) : "";
            icon_name = *name ? name : "battery-symbolic";
            if (percentage && g_variant_is_of_type(percentage, G_VARIANT_TYPE_DOUBLE)) {
                gdouble level = g_variant_get_double(percentage);
                if (level >= 0.0 && level <= 100.0) {
                    indicator->battery_level = (gint)(level + 0.5);
                    charge = g_strdup_printf("%d%%", indicator->battery_level);
                    label = charge;
                    charge_tooltip = g_strdup_printf("Vatra Panel · %s%s", charge,
                        indicator->charging ? " · Charging" : "");
                    tooltip = charge_tooltip;
                }
            }
        } else {
            icon_name = "ac-adapter-symbolic";
            label = "AC";
            tooltip = "Vatra — No battery detected";
        }
    }

    gchar *names[] = { (gchar *)icon_name, "battery-symbolic", "battery", NULL };
    g_autoptr(GIcon) image_icon = g_themed_icon_new_from_names(names, -1);
    gtk_image_set_from_gicon(GTK_IMAGE(indicator->image), image_icon, GTK_ICON_SIZE_MENU);
    gtk_label_set_text(GTK_LABEL(indicator->label), label);
    gtk_widget_set_visible(indicator->label, indicator->percentage_style == PERCENT_BESIDE
                           && indicator->battery_level >= 0);
    gtk_widget_set_visible(indicator->image, indicator->battery_level < 0);
    gtk_widget_set_visible(indicator->art, indicator->battery_level >= 0);
    gtk_widget_queue_draw(indicator->art);
    gtk_widget_set_tooltip_text(button, tooltip);
}

static void battery_changed(GDBusProxy *device, GVariant *changed,
                            const gchar *const *invalidated, GtkWidget *button)
{
    (void)device;
    (void)changed;
    (void)invalidated;
    update_battery(button);
}

static void battery_owner_changed(GObject *device, GParamSpec *property, GtkWidget *button)
{
    (void)device;
    (void)property;
    update_battery(button);
}

static void battery_ready(GObject *source, GAsyncResult *result, gpointer data)
{
    (void)source;
    GWeakRef *button_ref = data;
    g_autoptr(GtkWidget) button = g_weak_ref_get(button_ref);
    g_weak_ref_clear(button_ref);
    g_free(button_ref);

    g_autoptr(GDBusProxy) proxy = g_dbus_proxy_new_for_bus_finish(result, NULL);
    if (!button || !proxy)
        return;

    BatteryIndicator *indicator = g_object_get_data(G_OBJECT(button), "vatra-indicator");
    indicator->device = g_steal_pointer(&proxy);
    g_signal_connect_object(indicator->device, "g-properties-changed",
                            G_CALLBACK(battery_changed), button, 0);
    g_signal_connect_object(indicator->device, "notify::g-name-owner",
                            G_CALLBACK(battery_owner_changed), button, 0);
    update_battery(button);
}

static void launch_vatra(const gchar *option, const gchar *value)
{
    gchar *argv[] = { "/usr/bin/vatra", (gchar *)option, (gchar *)value, NULL };
    g_autoptr(GError) error = NULL;
    if (!g_spawn_async(NULL, argv, NULL, G_SPAWN_DEFAULT, NULL, NULL, NULL, &error))
        g_warning("Vatra: %s", error->message);
}

static void open_vatra(GtkWidget *widget, gpointer data)
{
    (void)widget;
    (void)data;
    launch_vatra(NULL, NULL);
}

static void open_indicator_settings(GtkMenuItem *item, gpointer data)
{
    (void)item;
    (void)data;
    launch_vatra("--indicator-settings", NULL);
}

static void request_power_mode(GtkMenuItem *item, gpointer data)
{
    (void)item;
    launch_vatra("--power-mode", data);
}

static void quit_vatra(GtkMenuItem *item, gpointer data)
{
    (void)item;
    (void)data;
    launch_vatra("--quit", NULL);
}

static void add_panel_menu(XfcePanelPlugin *plugin)
{
    static const struct { const gchar *label; const gchar *id; } modes[] = {
        { "Automatic", "automatic" },
        { "Balanced", "balanced" },
        { "Performance", "performance" },
        { "Power Saver", "power-saver" },
    };

    GtkWidget *open = gtk_menu_item_new_with_label("Open Vatra");
    g_signal_connect(open, "activate", G_CALLBACK(open_vatra), NULL);
    gtk_widget_show(open);
    xfce_panel_plugin_menu_insert_item(plugin, GTK_MENU_ITEM(open));

    GtkWidget *power = gtk_menu_item_new_with_label("Power modes");
    GtkWidget *submenu = gtk_menu_new();
    for (guint index = 0; index < G_N_ELEMENTS(modes); index++) {
        GtkWidget *item = gtk_menu_item_new_with_label(modes[index].label);
        g_signal_connect(item, "activate", G_CALLBACK(request_power_mode), (gpointer)modes[index].id);
        gtk_menu_shell_append(GTK_MENU_SHELL(submenu), item);
    }
    gtk_menu_item_set_submenu(GTK_MENU_ITEM(power), submenu);
    gtk_widget_show_all(power);
    xfce_panel_plugin_menu_insert_item(plugin, GTK_MENU_ITEM(power));

    GtkWidget *settings = gtk_menu_item_new_with_label("Indicator settings");
    g_signal_connect(settings, "activate", G_CALLBACK(open_indicator_settings), NULL);
    gtk_widget_show(settings);
    xfce_panel_plugin_menu_insert_item(plugin, GTK_MENU_ITEM(settings));

    GtkWidget *quit = gtk_menu_item_new_with_label("Quit Vatra");
    g_signal_connect(quit, "activate", G_CALLBACK(quit_vatra), NULL);
    gtk_widget_show(quit);
    xfce_panel_plugin_menu_insert_item(plugin, GTK_MENU_ITEM(quit));
}

static void update_layout(XfcePanelPlugin *plugin, GParamSpec *property, GtkWidget *button)
{
    (void)property;
    BatteryIndicator *indicator = g_object_get_data(G_OBJECT(button), "vatra-indicator");
    gboolean vertical = xfce_panel_plugin_get_mode(plugin) == XFCE_PANEL_PLUGIN_MODE_VERTICAL;
    gtk_orientable_set_orientation(GTK_ORIENTABLE(indicator->box),
                                   vertical ? GTK_ORIENTATION_VERTICAL : GTK_ORIENTATION_HORIZONTAL);
    indicator->icon_size = MAX(1, xfce_panel_plugin_get_icon_size(plugin));
    gtk_image_set_pixel_size(GTK_IMAGE(indicator->image), indicator->icon_size);
    resize_art(indicator);
}

static void update_visibility(GtkWidget *button)
{
    BatteryIndicator *indicator = g_object_get_data(G_OBJECT(button), "vatra-indicator");
    g_autofree gchar *choice_path = g_build_filename(indicator->settings_dir, "indicator", NULL);
    g_autofree gchar *legacy_path = g_build_filename(indicator->settings_dir, "tray.enabled", NULL);
    g_autofree gchar *choice = NULL;
    gboolean visible = !g_file_test(legacy_path, G_FILE_TEST_IS_REGULAR);
    if (g_file_get_contents(choice_path, &choice, NULL, NULL)) {
        g_strstrip(choice);
        if (g_str_equal(choice, "xfce-panel"))
            visible = TRUE;
        else if (g_str_equal(choice, "tray") || g_str_equal(choice, "off"))
            visible = FALSE;
    }
    gtk_widget_set_visible(button, visible);
}

static void update_appearance(GtkWidget *button)
{
    BatteryIndicator *indicator = g_object_get_data(G_OBJECT(button), "vatra-indicator");
    static const gchar *const accents[] = { "#ffb16b", "#82c2ff", "#70deb5", "#cdaaff" };
    g_autofree gchar *path = g_build_filename(indicator->settings_dir, "appearance", NULL);
    g_autofree gchar *contents = NULL;
    gdk_rgba_parse(&indicator->accent, accents[0]);
    indicator->percentage_style = PERCENT_TOOLTIP;
    indicator->charging_bolt = BOLT_ACCENT;

    if (g_file_get_contents(path, &contents, NULL, NULL)) {
        gchar *legacy = g_strstrip(contents);
        if (legacy[0] >= '0' && legacy[0] <= '3' && legacy[1] == '\0') {
            gdk_rgba_parse(&indicator->accent, accents[legacy[0] - '0']);
        } else {
            g_autofree gchar *text = g_strconcat("[Appearance]\n", contents, NULL);
            g_autoptr(GKeyFile) style = g_key_file_new();
            if (g_key_file_load_from_data(style, text, -1, G_KEY_FILE_NONE, NULL)) {
                g_autofree gchar *accent = g_key_file_get_string(style, "Appearance", "accent", NULL);
                if (accent && strlen(accent) == 6) {
                    g_autofree gchar *color = g_strconcat("#", accent, NULL);
                    gdk_rgba_parse(&indicator->accent, color);
                }
                gint percent = g_key_file_get_integer(style, "Appearance", "percent", NULL);
                if (percent >= PERCENT_TOOLTIP && percent <= PERCENT_BESIDE)
                    indicator->percentage_style = percent;
                gint bolt = g_key_file_get_integer(style, "Appearance", "bolt", NULL);
                if (bolt >= BOLT_ACCENT && bolt <= BOLT_HIDDEN)
                    indicator->charging_bolt = bolt;
            }
        }
    }
    resize_art(indicator);
    update_battery(button);
}

static void indicator_changed(GFileMonitor *monitor, GFile *file, GFile *other_file,
                              GFileMonitorEvent event, GtkWidget *button)
{
    (void)monitor;
    (void)file;
    (void)other_file;
    (void)event;
    update_appearance(button);
    update_visibility(button);
}

static void monitor_visibility(BatteryIndicator *indicator, GtkWidget *button)
{
    const gchar *config = g_getenv("XDG_CONFIG_HOME");
    g_autofree gchar *fallback = NULL;
    if (!config || !g_path_is_absolute(config)) {
        fallback = g_build_filename(g_get_home_dir(), ".config", NULL);
        config = fallback;
    }
    indicator->settings_dir = g_build_filename(config, "vatra", NULL);
    if (g_mkdir_with_parents(indicator->settings_dir, 0700) == 0) {
        g_autoptr(GFile) directory = g_file_new_for_path(indicator->settings_dir);
        indicator->settings_monitor = g_file_monitor_directory(directory, G_FILE_MONITOR_NONE,
                                                                NULL, NULL);
        if (indicator->settings_monitor)
            g_signal_connect_object(indicator->settings_monitor, "changed",
                                    G_CALLBACK(indicator_changed), button, 0);
    }
    update_appearance(button);
    update_visibility(button);
}

static void indicator_mapped(GtkWidget *button, BatteryIndicator *indicator)
{
    (void)button;
    if (indicator->visible_owner == 0) {
        indicator->visible_owner = g_bus_own_name(G_BUS_TYPE_SESSION,
            "io.github.VladCraft228.VatraPanel", G_BUS_NAME_OWNER_FLAGS_NONE,
            NULL, NULL, NULL, NULL, NULL);
    }
}

static void stop_reporting_visibility(BatteryIndicator *indicator)
{
    if (indicator->visible_owner != 0) {
        g_bus_unown_name(indicator->visible_owner);
        indicator->visible_owner = 0;
    }
}

static void indicator_unmapped(GtkWidget *button, BatteryIndicator *indicator)
{
    (void)button;
    stop_reporting_visibility(indicator);
}

static void destroy_indicator(gpointer data)
{
    BatteryIndicator *indicator = data;
    stop_reporting_visibility(indicator);
    g_clear_object(&indicator->device);
    g_clear_object(&indicator->settings_monitor);
    g_free(indicator->settings_dir);
    g_free(indicator);
}

static void construct_panel(XfcePanelPlugin *plugin)
{
    BatteryIndicator *indicator = g_new0(BatteryIndicator, 1);
    GtkWidget *button = xfce_panel_create_button();
    indicator->box = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 4);
    indicator->image = gtk_image_new();
    indicator->art = gtk_drawing_area_new();
    gtk_widget_set_halign(indicator->art, GTK_ALIGN_CENTER);
    gtk_widget_set_valign(indicator->art, GTK_ALIGN_CENTER);
    indicator->label = gtk_label_new(NULL);
    gtk_widget_set_no_show_all(indicator->image, TRUE);
    gtk_widget_set_no_show_all(indicator->art, TRUE);
    gtk_widget_set_no_show_all(indicator->label, TRUE);
    g_signal_connect(indicator->art, "draw", G_CALLBACK(draw_battery), indicator);
    gtk_label_set_width_chars(GTK_LABEL(indicator->label), 4);
    gtk_container_set_border_width(GTK_CONTAINER(indicator->box), 2);
    gtk_box_pack_start(GTK_BOX(indicator->box), indicator->image, FALSE, FALSE, 0);
    gtk_box_pack_start(GTK_BOX(indicator->box), indicator->art, FALSE, FALSE, 0);
    gtk_box_pack_start(GTK_BOX(indicator->box), indicator->label, FALSE, FALSE, 0);
    gtk_container_add(GTK_CONTAINER(button), indicator->box);
    gtk_container_add(GTK_CONTAINER(plugin), button);
    g_object_set_data_full(G_OBJECT(button), "vatra-indicator", indicator, destroy_indicator);
    xfce_panel_plugin_add_action_widget(plugin, button);
    add_panel_menu(plugin);
    g_signal_connect(button, "clicked", G_CALLBACK(open_vatra), NULL);
    g_signal_connect_object(plugin, "notify::mode", G_CALLBACK(update_layout), button, 0);
    g_signal_connect_object(plugin, "notify::size", G_CALLBACK(update_layout), button, 0);
    g_signal_connect_object(plugin, "notify::icon-size", G_CALLBACK(update_layout), button, 0);
    update_battery(button);
    update_layout(plugin, NULL, button);
    g_signal_connect(button, "map", G_CALLBACK(indicator_mapped), indicator);
    g_signal_connect(button, "unmap", G_CALLBACK(indicator_unmapped), indicator);
    gtk_widget_set_no_show_all(button, TRUE);
    gtk_widget_show_all(indicator->box);
    monitor_visibility(indicator, button);

    GWeakRef *button_ref = g_new0(GWeakRef, 1);
    g_weak_ref_init(button_ref, button);
    g_dbus_proxy_new_for_bus(G_BUS_TYPE_SYSTEM, G_DBUS_PROXY_FLAGS_GET_INVALIDATED_PROPERTIES,
                            NULL, "org.freedesktop.UPower",
                            "/org/freedesktop/UPower/devices/DisplayDevice",
                            "org.freedesktop.UPower.Device", NULL, battery_ready, button_ref);
}

XFCE_PANEL_PLUGIN_REGISTER(construct_panel)
