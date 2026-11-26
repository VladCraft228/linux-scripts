/* SPDX-License-Identifier: MIT */
#include <gio/gio.h>
#include <gtk/gtk.h>
#include <libxfce4panel/libxfce4panel.h>

typedef struct {
    GtkWidget *box;
    GtkWidget *image;
    GtkWidget *label;
    GDBusProxy *device;
} BatteryIndicator;

static void update_battery(GtkWidget *button)
{
    BatteryIndicator *indicator = g_object_get_data(G_OBJECT(button), "vatra-indicator");
    GDBusProxy *device = indicator->device;
    g_autofree gchar *owner = device ? g_dbus_proxy_get_name_owner(device) : NULL;
    g_autoptr(GVariant) present = owner ? g_dbus_proxy_get_cached_property(device, "IsPresent") : NULL;
    g_autoptr(GVariant) percentage = owner ? g_dbus_proxy_get_cached_property(device, "Percentage") : NULL;
    g_autoptr(GVariant) icon = owner ? g_dbus_proxy_get_cached_property(device, "IconName") : NULL;

    const gchar *icon_name = "battery-missing-symbolic";
    const gchar *label = "--";
    const gchar *tooltip = "Vatra — Battery status unavailable";
    g_autofree gchar *charge = NULL;
    if (present && g_variant_is_of_type(present, G_VARIANT_TYPE_BOOLEAN)) {
        if (g_variant_get_boolean(present)) {
            const gchar *name = icon && g_variant_is_of_type(icon, G_VARIANT_TYPE_STRING)
                ? g_variant_get_string(icon, NULL) : "";
            icon_name = *name ? name : "battery-symbolic";
            if (percentage && g_variant_is_of_type(percentage, G_VARIANT_TYPE_DOUBLE)) {
                gdouble level = g_variant_get_double(percentage);
                if (level >= 0.0 && level <= 100.0) {
                    charge = g_strdup_printf("%d%%", (gint)(level + 0.5));
                    label = charge;
                    tooltip = charge;
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

static void open_vatra(GtkButton *button, gpointer data)
{
    (void)button;
    (void)data;
    gchar *argv[] = { "/usr/bin/vatra", NULL };
    g_autoptr(GError) error = NULL;
    if (!g_spawn_async(NULL, argv, NULL, G_SPAWN_DEFAULT, NULL, NULL, NULL, &error))
        g_warning("Vatra: %s", error->message);
}

static void update_layout(XfcePanelPlugin *plugin, GParamSpec *property, GtkWidget *button)
{
    (void)property;
    BatteryIndicator *indicator = g_object_get_data(G_OBJECT(button), "vatra-indicator");
    gboolean vertical = xfce_panel_plugin_get_mode(plugin) == XFCE_PANEL_PLUGIN_MODE_VERTICAL;
    gtk_orientable_set_orientation(GTK_ORIENTABLE(indicator->box),
                                   vertical ? GTK_ORIENTATION_VERTICAL : GTK_ORIENTATION_HORIZONTAL);
    gtk_image_set_pixel_size(GTK_IMAGE(indicator->image),
                             MAX(1, xfce_panel_plugin_get_icon_size(plugin)));
}

static void destroy_indicator(gpointer data)
{
    BatteryIndicator *indicator = data;
    g_clear_object(&indicator->device);
    g_free(indicator);
}

static void construct_panel(XfcePanelPlugin *plugin)
{
    BatteryIndicator *indicator = g_new0(BatteryIndicator, 1);
    GtkWidget *button = xfce_panel_create_button();
    indicator->box = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 4);
    indicator->image = gtk_image_new();
    indicator->label = gtk_label_new(NULL);
    gtk_label_set_width_chars(GTK_LABEL(indicator->label), 4);
    gtk_container_set_border_width(GTK_CONTAINER(indicator->box), 2);
    gtk_box_pack_start(GTK_BOX(indicator->box), indicator->image, FALSE, FALSE, 0);
    gtk_box_pack_start(GTK_BOX(indicator->box), indicator->label, FALSE, FALSE, 0);
    gtk_container_add(GTK_CONTAINER(button), indicator->box);
    gtk_container_add(GTK_CONTAINER(plugin), button);
    g_object_set_data_full(G_OBJECT(button), "vatra-indicator", indicator, destroy_indicator);
    xfce_panel_plugin_add_action_widget(plugin, button);
    g_signal_connect(button, "clicked", G_CALLBACK(open_vatra), NULL);
    g_signal_connect_object(plugin, "notify::mode", G_CALLBACK(update_layout), button, 0);
    g_signal_connect_object(plugin, "notify::size", G_CALLBACK(update_layout), button, 0);
    g_signal_connect_object(plugin, "notify::icon-size", G_CALLBACK(update_layout), button, 0);
    update_battery(button);
    update_layout(plugin, NULL, button);
    gtk_widget_show_all(button);

    GWeakRef *button_ref = g_new0(GWeakRef, 1);
    g_weak_ref_init(button_ref, button);
    g_dbus_proxy_new_for_bus(G_BUS_TYPE_SYSTEM, G_DBUS_PROXY_FLAGS_GET_INVALIDATED_PROPERTIES,
                            NULL, "org.freedesktop.UPower",
                            "/org/freedesktop/UPower/devices/DisplayDevice",
                            "org.freedesktop.UPower.Device", NULL, battery_ready, button_ref);
}

XFCE_PANEL_PLUGIN_REGISTER(construct_panel)
