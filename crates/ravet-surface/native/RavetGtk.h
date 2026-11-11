// GTK 3, WebKitGTK 4.1 and json-glib, resolved with dlopen on first use.
//
// The Linux editor sources call these libraries through the function pointers
// below instead of linking them, so a binary built on RAVET loads (and plays)
// where they are missing: in a Flatpak host whose runtime lacks WebKitGTK, or
// on a system without it. Only creating an editor or window then fails, and
// hosts show their generic controls.
//
// Include this after every other header: it redirects each name listed here to
// its pointer with a macro. Call ravet_gtk::load before the first GTK call. A
// function used without being listed stays an undefined symbol of the binary;
// ravet-webview's build script rejects that.
#pragma once
#include <gtk/gtk.h>
#include <gtk/gtkx.h>
#include <gdk/gdkx.h>
#include <webkit2/webkit2.h>
#include <json-glib/json-glib.h>

// GTK 3, GDK, GIO, GObject and GLib, resolved through libgtk-3.so.0.
#define RAVET_GTK_FUNCTIONS(X) \
    X(g_bytes_new) X(g_bytes_new_static) X(g_bytes_unref) X(g_error_free) \
    X(g_filename_from_uri) X(g_filename_to_uri) X(g_free) X(g_get_monotonic_time) \
    X(g_main_context_iteration) X(g_main_context_pending) X(g_memory_input_stream_new_from_bytes) \
    X(g_object_get_data) X(g_object_ref_sink) X(g_object_set_data) X(g_object_unref) \
    X(g_signal_connect_data) X(g_signal_handler_disconnect) X(g_signal_handlers_disconnect_matched) \
    X(g_source_attach) X(g_source_destroy) X(g_source_remove) X(g_source_set_callback) \
    X(g_source_unref) X(g_timeout_add_full) X(g_timeout_source_new) \
    X(g_type_check_instance_cast) X(g_type_check_instance_is_a) \
    X(gdk_display_get_default) X(gdk_set_allowed_backends) X(gdk_window_get_state) X(gdk_window_show) \
    X(gdk_x11_display_error_trap_pop_ignored) X(gdk_x11_display_error_trap_push) X(gdk_x11_display_get_type) \
    X(gtk_box_get_type) X(gtk_box_new) X(gtk_box_pack_start) X(gtk_button_new_with_label) \
    X(gtk_container_add) X(gtk_container_get_type) X(gtk_container_remove) \
    X(gtk_file_chooser_add_filter) X(gtk_file_chooser_dialog_new) X(gtk_file_chooser_get_filename) \
    X(gtk_file_chooser_get_type) X(gtk_file_filter_add_pattern) X(gtk_file_filter_new) \
    X(gtk_header_bar_get_type) X(gtk_header_bar_new) X(gtk_header_bar_pack_end) \
    X(gtk_header_bar_set_show_close_button) X(gtk_header_bar_set_title) X(gtk_init_check) \
    X(gtk_plug_new) X(gtk_socket_get_id) X(gtk_socket_get_type) X(gtk_socket_new) \
    X(gtk_widget_destroy) X(gtk_widget_get_allocation) X(gtk_widget_get_preferred_height) \
    X(gtk_widget_get_type) X(gtk_widget_get_visible) X(gtk_widget_get_window) X(gtk_widget_hide) \
    X(gtk_widget_realize) X(gtk_widget_set_hexpand) X(gtk_widget_set_size_request) \
    X(gtk_widget_set_vexpand) X(gtk_widget_show) X(gtk_widget_show_all) \
    X(gtk_window_close) X(gtk_window_get_titlebar) X(gtk_window_get_type) X(gtk_window_is_active) \
    X(gtk_window_new) X(gtk_window_resize) X(gtk_window_set_default_size) \
    X(gtk_window_set_geometry_hints) X(gtk_window_set_position) X(gtk_window_set_title) \
    X(gtk_window_set_titlebar)

// WebKitGTK 4.1 (2.40+) and JavaScriptCore, resolved through libwebkit2gtk-4.1.so.0.
#define RAVET_WEBKIT_FUNCTIONS(X) \
    X(jsc_value_is_boolean) X(jsc_value_to_boolean) X(jsc_value_to_json) \
    X(webkit_javascript_result_get_js_value) X(webkit_navigation_policy_decision_get_request) \
    X(webkit_navigation_policy_decision_get_type) X(webkit_policy_decision_ignore) \
    X(webkit_security_manager_register_uri_scheme_as_cors_enabled) \
    X(webkit_security_manager_register_uri_scheme_as_secure) \
    X(webkit_settings_set_hardware_acceleration_policy) X(webkit_uri_request_get_uri) \
    X(webkit_uri_scheme_request_finish_with_response) X(webkit_uri_scheme_request_get_http_method) \
    X(webkit_uri_scheme_request_get_uri) X(webkit_uri_scheme_request_get_web_view) \
    X(webkit_uri_scheme_response_new) X(webkit_uri_scheme_response_set_content_type) \
    X(webkit_uri_scheme_response_set_status) X(webkit_user_content_manager_add_script) \
    X(webkit_user_content_manager_new) X(webkit_user_content_manager_register_script_message_handler) \
    X(webkit_user_content_manager_unregister_script_message_handler) X(webkit_user_script_new) \
    X(webkit_user_script_unref) X(webkit_web_context_get_default) \
    X(webkit_web_context_get_security_manager) X(webkit_web_context_register_uri_scheme) \
    X(webkit_web_view_evaluate_javascript) X(webkit_web_view_evaluate_javascript_finish) \
    X(webkit_web_view_get_settings) X(webkit_web_view_get_type) X(webkit_web_view_load_html) \
    X(webkit_web_view_load_uri) X(webkit_web_view_new_with_user_content_manager) \
    X(webkit_web_view_set_background_color) X(webkit_web_view_stop_loading)

// json-glib, resolved through libjson-glib-1.0.so.0.
#define RAVET_JSON_FUNCTIONS(X) \
    X(json_node_get_node_type) X(json_parser_get_root) X(json_parser_load_from_data) X(json_parser_new)

namespace ravet_gtk {
#define RAVET_GTK_FIELD(name) decltype(&::name) name;
struct Api {
    RAVET_GTK_FUNCTIONS(RAVET_GTK_FIELD)
    RAVET_WEBKIT_FUNCTIONS(RAVET_GTK_FIELD)
    RAVET_JSON_FUNCTIONS(RAVET_GTK_FIELD)
};
#undef RAVET_GTK_FIELD
// Hidden: two RAVET binaries in one host each keep their own table.
__attribute__((visibility("hidden"))) extern Api api;
enum class Part { gtk, web };
// Loads GTK (Part::gtk) or GTK plus WebKitGTK and json-glib (Part::web) once
// per process; false when a library or function is missing. Thread-safe.
__attribute__((visibility("hidden"))) bool load(Part part) noexcept;
}

#ifndef RAVET_GTK_NO_REDIRECT
// GLib's own macros for some of these names (g_free, g_object_ref_sink) are
// replaced; their checked forms would reference symbols outside the table.
#undef g_bytes_new
#define g_bytes_new (ravet_gtk::api.g_bytes_new)
#undef g_bytes_new_static
#define g_bytes_new_static (ravet_gtk::api.g_bytes_new_static)
#undef g_bytes_unref
#define g_bytes_unref (ravet_gtk::api.g_bytes_unref)
#undef g_error_free
#define g_error_free (ravet_gtk::api.g_error_free)
#undef g_filename_from_uri
#define g_filename_from_uri (ravet_gtk::api.g_filename_from_uri)
#undef g_filename_to_uri
#define g_filename_to_uri (ravet_gtk::api.g_filename_to_uri)
#undef g_free
#define g_free (ravet_gtk::api.g_free)
#undef g_get_monotonic_time
#define g_get_monotonic_time (ravet_gtk::api.g_get_monotonic_time)
#undef g_main_context_iteration
#define g_main_context_iteration (ravet_gtk::api.g_main_context_iteration)
#undef g_main_context_pending
#define g_main_context_pending (ravet_gtk::api.g_main_context_pending)
#undef g_memory_input_stream_new_from_bytes
#define g_memory_input_stream_new_from_bytes (ravet_gtk::api.g_memory_input_stream_new_from_bytes)
#undef g_object_get_data
#define g_object_get_data (ravet_gtk::api.g_object_get_data)
#undef g_object_ref_sink
#define g_object_ref_sink (ravet_gtk::api.g_object_ref_sink)
#undef g_object_set_data
#define g_object_set_data (ravet_gtk::api.g_object_set_data)
#undef g_object_unref
#define g_object_unref (ravet_gtk::api.g_object_unref)
#undef g_signal_connect_data
#define g_signal_connect_data (ravet_gtk::api.g_signal_connect_data)
#undef g_signal_handler_disconnect
#define g_signal_handler_disconnect (ravet_gtk::api.g_signal_handler_disconnect)
#undef g_signal_handlers_disconnect_matched
#define g_signal_handlers_disconnect_matched (ravet_gtk::api.g_signal_handlers_disconnect_matched)
#undef g_source_attach
#define g_source_attach (ravet_gtk::api.g_source_attach)
#undef g_source_destroy
#define g_source_destroy (ravet_gtk::api.g_source_destroy)
#undef g_source_remove
#define g_source_remove (ravet_gtk::api.g_source_remove)
#undef g_source_set_callback
#define g_source_set_callback (ravet_gtk::api.g_source_set_callback)
#undef g_source_unref
#define g_source_unref (ravet_gtk::api.g_source_unref)
#undef g_timeout_add_full
#define g_timeout_add_full (ravet_gtk::api.g_timeout_add_full)
#undef g_timeout_source_new
#define g_timeout_source_new (ravet_gtk::api.g_timeout_source_new)
#undef g_type_check_instance_cast
#define g_type_check_instance_cast (ravet_gtk::api.g_type_check_instance_cast)
#undef g_type_check_instance_is_a
#define g_type_check_instance_is_a (ravet_gtk::api.g_type_check_instance_is_a)
#undef gdk_display_get_default
#define gdk_display_get_default (ravet_gtk::api.gdk_display_get_default)
#undef gdk_set_allowed_backends
#define gdk_set_allowed_backends (ravet_gtk::api.gdk_set_allowed_backends)
#undef gdk_window_get_state
#define gdk_window_get_state (ravet_gtk::api.gdk_window_get_state)
#undef gdk_window_show
#define gdk_window_show (ravet_gtk::api.gdk_window_show)
#undef gdk_x11_display_error_trap_pop_ignored
#define gdk_x11_display_error_trap_pop_ignored (ravet_gtk::api.gdk_x11_display_error_trap_pop_ignored)
#undef gdk_x11_display_error_trap_push
#define gdk_x11_display_error_trap_push (ravet_gtk::api.gdk_x11_display_error_trap_push)
#undef gdk_x11_display_get_type
#define gdk_x11_display_get_type (ravet_gtk::api.gdk_x11_display_get_type)
#undef gtk_box_get_type
#define gtk_box_get_type (ravet_gtk::api.gtk_box_get_type)
#undef gtk_box_new
#define gtk_box_new (ravet_gtk::api.gtk_box_new)
#undef gtk_box_pack_start
#define gtk_box_pack_start (ravet_gtk::api.gtk_box_pack_start)
#undef gtk_button_new_with_label
#define gtk_button_new_with_label (ravet_gtk::api.gtk_button_new_with_label)
#undef gtk_container_add
#define gtk_container_add (ravet_gtk::api.gtk_container_add)
#undef gtk_container_get_type
#define gtk_container_get_type (ravet_gtk::api.gtk_container_get_type)
#undef gtk_container_remove
#define gtk_container_remove (ravet_gtk::api.gtk_container_remove)
#undef gtk_file_chooser_add_filter
#define gtk_file_chooser_add_filter (ravet_gtk::api.gtk_file_chooser_add_filter)
#undef gtk_file_chooser_dialog_new
#define gtk_file_chooser_dialog_new (ravet_gtk::api.gtk_file_chooser_dialog_new)
#undef gtk_file_chooser_get_filename
#define gtk_file_chooser_get_filename (ravet_gtk::api.gtk_file_chooser_get_filename)
#undef gtk_file_chooser_get_type
#define gtk_file_chooser_get_type (ravet_gtk::api.gtk_file_chooser_get_type)
#undef gtk_file_filter_add_pattern
#define gtk_file_filter_add_pattern (ravet_gtk::api.gtk_file_filter_add_pattern)
#undef gtk_file_filter_new
#define gtk_file_filter_new (ravet_gtk::api.gtk_file_filter_new)
#undef gtk_header_bar_get_type
#define gtk_header_bar_get_type (ravet_gtk::api.gtk_header_bar_get_type)
#undef gtk_header_bar_new
#define gtk_header_bar_new (ravet_gtk::api.gtk_header_bar_new)
#undef gtk_header_bar_pack_end
#define gtk_header_bar_pack_end (ravet_gtk::api.gtk_header_bar_pack_end)
#undef gtk_header_bar_set_show_close_button
#define gtk_header_bar_set_show_close_button (ravet_gtk::api.gtk_header_bar_set_show_close_button)
#undef gtk_header_bar_set_title
#define gtk_header_bar_set_title (ravet_gtk::api.gtk_header_bar_set_title)
#undef gtk_init_check
#define gtk_init_check (ravet_gtk::api.gtk_init_check)
#undef gtk_plug_new
#define gtk_plug_new (ravet_gtk::api.gtk_plug_new)
#undef gtk_socket_get_id
#define gtk_socket_get_id (ravet_gtk::api.gtk_socket_get_id)
#undef gtk_socket_get_type
#define gtk_socket_get_type (ravet_gtk::api.gtk_socket_get_type)
#undef gtk_socket_new
#define gtk_socket_new (ravet_gtk::api.gtk_socket_new)
#undef gtk_widget_destroy
#define gtk_widget_destroy (ravet_gtk::api.gtk_widget_destroy)
#undef gtk_widget_get_allocation
#define gtk_widget_get_allocation (ravet_gtk::api.gtk_widget_get_allocation)
#undef gtk_widget_get_preferred_height
#define gtk_widget_get_preferred_height (ravet_gtk::api.gtk_widget_get_preferred_height)
#undef gtk_widget_get_type
#define gtk_widget_get_type (ravet_gtk::api.gtk_widget_get_type)
#undef gtk_widget_get_visible
#define gtk_widget_get_visible (ravet_gtk::api.gtk_widget_get_visible)
#undef gtk_widget_get_window
#define gtk_widget_get_window (ravet_gtk::api.gtk_widget_get_window)
#undef gtk_widget_hide
#define gtk_widget_hide (ravet_gtk::api.gtk_widget_hide)
#undef gtk_widget_realize
#define gtk_widget_realize (ravet_gtk::api.gtk_widget_realize)
#undef gtk_widget_set_hexpand
#define gtk_widget_set_hexpand (ravet_gtk::api.gtk_widget_set_hexpand)
#undef gtk_widget_set_size_request
#define gtk_widget_set_size_request (ravet_gtk::api.gtk_widget_set_size_request)
#undef gtk_widget_set_vexpand
#define gtk_widget_set_vexpand (ravet_gtk::api.gtk_widget_set_vexpand)
#undef gtk_widget_show
#define gtk_widget_show (ravet_gtk::api.gtk_widget_show)
#undef gtk_widget_show_all
#define gtk_widget_show_all (ravet_gtk::api.gtk_widget_show_all)
#undef gtk_window_close
#define gtk_window_close (ravet_gtk::api.gtk_window_close)
#undef gtk_window_get_titlebar
#define gtk_window_get_titlebar (ravet_gtk::api.gtk_window_get_titlebar)
#undef gtk_window_get_type
#define gtk_window_get_type (ravet_gtk::api.gtk_window_get_type)
#undef gtk_window_is_active
#define gtk_window_is_active (ravet_gtk::api.gtk_window_is_active)
#undef gtk_window_new
#define gtk_window_new (ravet_gtk::api.gtk_window_new)
#undef gtk_window_resize
#define gtk_window_resize (ravet_gtk::api.gtk_window_resize)
#undef gtk_window_set_default_size
#define gtk_window_set_default_size (ravet_gtk::api.gtk_window_set_default_size)
#undef gtk_window_set_geometry_hints
#define gtk_window_set_geometry_hints (ravet_gtk::api.gtk_window_set_geometry_hints)
#undef gtk_window_set_position
#define gtk_window_set_position (ravet_gtk::api.gtk_window_set_position)
#undef gtk_window_set_title
#define gtk_window_set_title (ravet_gtk::api.gtk_window_set_title)
#undef gtk_window_set_titlebar
#define gtk_window_set_titlebar (ravet_gtk::api.gtk_window_set_titlebar)
#undef jsc_value_is_boolean
#define jsc_value_is_boolean (ravet_gtk::api.jsc_value_is_boolean)
#undef jsc_value_to_boolean
#define jsc_value_to_boolean (ravet_gtk::api.jsc_value_to_boolean)
#undef jsc_value_to_json
#define jsc_value_to_json (ravet_gtk::api.jsc_value_to_json)
#undef webkit_javascript_result_get_js_value
#define webkit_javascript_result_get_js_value (ravet_gtk::api.webkit_javascript_result_get_js_value)
#undef webkit_navigation_policy_decision_get_request
#define webkit_navigation_policy_decision_get_request (ravet_gtk::api.webkit_navigation_policy_decision_get_request)
#undef webkit_navigation_policy_decision_get_type
#define webkit_navigation_policy_decision_get_type (ravet_gtk::api.webkit_navigation_policy_decision_get_type)
#undef webkit_policy_decision_ignore
#define webkit_policy_decision_ignore (ravet_gtk::api.webkit_policy_decision_ignore)
#undef webkit_security_manager_register_uri_scheme_as_cors_enabled
#define webkit_security_manager_register_uri_scheme_as_cors_enabled (ravet_gtk::api.webkit_security_manager_register_uri_scheme_as_cors_enabled)
#undef webkit_security_manager_register_uri_scheme_as_secure
#define webkit_security_manager_register_uri_scheme_as_secure (ravet_gtk::api.webkit_security_manager_register_uri_scheme_as_secure)
#undef webkit_settings_set_hardware_acceleration_policy
#define webkit_settings_set_hardware_acceleration_policy (ravet_gtk::api.webkit_settings_set_hardware_acceleration_policy)
#undef webkit_uri_request_get_uri
#define webkit_uri_request_get_uri (ravet_gtk::api.webkit_uri_request_get_uri)
#undef webkit_uri_scheme_request_finish_with_response
#define webkit_uri_scheme_request_finish_with_response (ravet_gtk::api.webkit_uri_scheme_request_finish_with_response)
#undef webkit_uri_scheme_request_get_http_method
#define webkit_uri_scheme_request_get_http_method (ravet_gtk::api.webkit_uri_scheme_request_get_http_method)
#undef webkit_uri_scheme_request_get_uri
#define webkit_uri_scheme_request_get_uri (ravet_gtk::api.webkit_uri_scheme_request_get_uri)
#undef webkit_uri_scheme_request_get_web_view
#define webkit_uri_scheme_request_get_web_view (ravet_gtk::api.webkit_uri_scheme_request_get_web_view)
#undef webkit_uri_scheme_response_new
#define webkit_uri_scheme_response_new (ravet_gtk::api.webkit_uri_scheme_response_new)
#undef webkit_uri_scheme_response_set_content_type
#define webkit_uri_scheme_response_set_content_type (ravet_gtk::api.webkit_uri_scheme_response_set_content_type)
#undef webkit_uri_scheme_response_set_status
#define webkit_uri_scheme_response_set_status (ravet_gtk::api.webkit_uri_scheme_response_set_status)
#undef webkit_user_content_manager_add_script
#define webkit_user_content_manager_add_script (ravet_gtk::api.webkit_user_content_manager_add_script)
#undef webkit_user_content_manager_new
#define webkit_user_content_manager_new (ravet_gtk::api.webkit_user_content_manager_new)
#undef webkit_user_content_manager_register_script_message_handler
#define webkit_user_content_manager_register_script_message_handler (ravet_gtk::api.webkit_user_content_manager_register_script_message_handler)
#undef webkit_user_content_manager_unregister_script_message_handler
#define webkit_user_content_manager_unregister_script_message_handler (ravet_gtk::api.webkit_user_content_manager_unregister_script_message_handler)
#undef webkit_user_script_new
#define webkit_user_script_new (ravet_gtk::api.webkit_user_script_new)
#undef webkit_user_script_unref
#define webkit_user_script_unref (ravet_gtk::api.webkit_user_script_unref)
#undef webkit_web_context_get_default
#define webkit_web_context_get_default (ravet_gtk::api.webkit_web_context_get_default)
#undef webkit_web_context_get_security_manager
#define webkit_web_context_get_security_manager (ravet_gtk::api.webkit_web_context_get_security_manager)
#undef webkit_web_context_register_uri_scheme
#define webkit_web_context_register_uri_scheme (ravet_gtk::api.webkit_web_context_register_uri_scheme)
#undef webkit_web_view_evaluate_javascript
#define webkit_web_view_evaluate_javascript (ravet_gtk::api.webkit_web_view_evaluate_javascript)
#undef webkit_web_view_evaluate_javascript_finish
#define webkit_web_view_evaluate_javascript_finish (ravet_gtk::api.webkit_web_view_evaluate_javascript_finish)
#undef webkit_web_view_get_settings
#define webkit_web_view_get_settings (ravet_gtk::api.webkit_web_view_get_settings)
#undef webkit_web_view_get_type
#define webkit_web_view_get_type (ravet_gtk::api.webkit_web_view_get_type)
#undef webkit_web_view_load_html
#define webkit_web_view_load_html (ravet_gtk::api.webkit_web_view_load_html)
#undef webkit_web_view_load_uri
#define webkit_web_view_load_uri (ravet_gtk::api.webkit_web_view_load_uri)
#undef webkit_web_view_new_with_user_content_manager
#define webkit_web_view_new_with_user_content_manager (ravet_gtk::api.webkit_web_view_new_with_user_content_manager)
#undef webkit_web_view_set_background_color
#define webkit_web_view_set_background_color (ravet_gtk::api.webkit_web_view_set_background_color)
#undef webkit_web_view_stop_loading
#define webkit_web_view_stop_loading (ravet_gtk::api.webkit_web_view_stop_loading)
#undef json_node_get_node_type
#define json_node_get_node_type (ravet_gtk::api.json_node_get_node_type)
#undef json_parser_get_root
#define json_parser_get_root (ravet_gtk::api.json_parser_get_root)
#undef json_parser_load_from_data
#define json_parser_load_from_data (ravet_gtk::api.json_parser_load_from_data)
#undef json_parser_new
#define json_parser_new (ravet_gtk::api.json_parser_new)
#endif
