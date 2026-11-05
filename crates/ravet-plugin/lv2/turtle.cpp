// Generates manifest.ttl and plugin.ttl for a RAVET product from its
// RavetPluginEntryV1 table. Run at build time on the build platform:
//
//   ravet_lv2_turtle <product library> <bundle directory>
//       --binary <adapter file> [--ui-binary <ui file>] [--ui-class X11UI|CocoaUI|WindowsUI]
//       [--license <URI>]
//
// The port layout comes from RavetLv2Product.hpp, the same code the adapter
// uses at run time.
#include "RavetLv2Product.hpp"

#include <cmath>
#include <cstdio>
#include <fstream>
#include <iostream>
#include <sstream>
#include <string>

namespace {
using namespace ravet::lv2;

std::string literal(const std::string &text) {
    std::string out = "\"";
    for (unsigned char c : text) {
        switch (c) {
        case '"': out += "\\\""; break;
        case '\\': out += "\\\\"; break;
        case '\n': out += "\\n"; break;
        case '\r': out += "\\r"; break;
        case '\t': out += "\\t"; break;
        default:
            if (c < 0x20) { char buffer[8]; std::snprintf(buffer, sizeof buffer, "\\u%04X", c); out += buffer; }
            else out += char(c);
        }
    }
    return out + "\"";
}

bool validIri(const std::string &iri) {
    if (iri.empty() || iri.find(':') == std::string::npos) return false;
    for (unsigned char c : iri)
        if (c <= 0x20 || std::strchr("<>\"{}|^`\\", c)) return false;
    return true;
}

// Shortest decimal that round-trips the float a control port carries, always
// written as a Turtle decimal or double (never an integer literal).
std::string number(double value) {
    const float single = float(value);
    char buffer[64];
    for (int precision = 6; precision <= 9; ++precision) {
        std::snprintf(buffer, sizeof buffer, "%.*g", precision, double(single));
        if (std::strtof(buffer, nullptr) == single) break;
    }
    std::string text = buffer;
    if (text.find_first_of(".eE") == std::string::npos) text += ".0";
    else if (text.find('.') == std::string::npos && text.find_first_of("eE") != std::string::npos) {
        // "1e-05" is a valid Turtle double; keep it.
    }
    return text;
}

// LV2 units vocabulary for common unit strings; empty when no match.
std::string unitUri(const std::string &unit) {
    struct Pair { const char *text, *uri; };
    static const Pair pairs[] = {
        {"dB", "units:db"}, {"db", "units:db"}, {"Hz", "units:hz"}, {"kHz", "units:khz"}, {"MHz", "units:mhz"},
        {"ms", "units:ms"}, {"s", "units:s"}, {"min", "units:min"}, {"%", "units:pc"}, {"bpm", "units:bpm"},
        {"BPM", "units:bpm"}, {"st", "units:semitone12TET"}, {"semitones", "units:semitone12TET"},
        {"ct", "units:cent"}, {"cents", "units:cent"}, {"cent", "units:cent"}, {"oct", "units:oct"},
        {"°", "units:degree"}, {"deg", "units:degree"}, {"m", "units:m"}, {"cm", "units:cm"}, {"mm", "units:mm"},
        {"km", "units:km"}, {"beats", "units:beat"}, {"bars", "units:bar"}, {"frames", "units:frame"},
        {"samples", "units:frame"}, {"note", "units:midiNote"},
    };
    for (const auto &pair : pairs) if (unit == pair.text) return pair.uri;
    return {};
}

std::string uiUri(const std::string &plugin) { return plugin + (plugin.find('#') == std::string::npos ? "#ui" : "-ui"); }

void usage() {
    std::cerr << "usage: ravet_lv2_turtle <product library> <bundle directory> --binary <file> "
                 "[--ui-binary <file>] [--ui-class X11UI|CocoaUI|WindowsUI] [--license <URI>]\n";
}

} // namespace

int main(int argc, char **argv) {
    if (argc < 3) { usage(); return 2; }
    const std::string libraryPath = argv[1], directory = argv[2];
    std::string binary, uiBinary, uiClass, license;
    for (int i = 3; i + 1 < argc; i += 2) {
        const std::string option = argv[i], value = argv[i + 1];
        if (option == "--binary") binary = value;
        else if (option == "--ui-binary") uiBinary = value;
        else if (option == "--ui-class") uiClass = value;
        else if (option == "--license") license = value;
        else { usage(); return 2; }
    }
    if ((argc - 3) % 2 || binary.empty()) { usage(); return 2; }
    if (!uiBinary.empty() && uiClass != "X11UI" && uiClass != "CocoaUI" && uiClass != "WindowsUI") { usage(); return 2; }

    std::string error;
    const RavetPluginEntryV1 *entry = loadProduct(libraryPath, error);
    if (!entry) { std::cerr << "ravet_lv2_turtle: " << error << "\n"; return 1; }
    const auto &product = *entry->product;
    const std::string uri = product.lv2_uri;
    if (!validIri(uri)) { std::cerr << "ravet_lv2_turtle: invalid LV2 URI " << uri << "\n"; return 1; }
    if (!license.empty() && !validIri(license)) { std::cerr << "ravet_lv2_turtle: invalid licence URI\n"; return 1; }
    const bool editor = product.editor_width && product.editor_height && !uiBinary.empty();
    const std::string ui = uiUri(uri);

    // Version "major.minor.patch" → lv2:minorVersion major*1000+minor, lv2:microVersion patch.
    unsigned major = 0, minor = 0, patch = 0;
    std::sscanf(product.version ? product.version : "0.0.0", "%u.%u.%u", &major, &minor, &patch);

    {
        std::ofstream manifest(directory + "/manifest.ttl");
        manifest << "@prefix lv2: <http://lv2plug.in/ns/lv2core#> .\n"
                    "@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n"
                    "@prefix ui: <http://lv2plug.in/ns/extensions/ui#> .\n\n"
                 << "<" << uri << "> a lv2:Plugin ;\n  lv2:binary <" << binary << "> ;\n  rdfs:seeAlso <plugin.ttl> .\n";
        if (editor)
            manifest << "\n<" << ui << "> a ui:" << uiClass << " ;\n  ui:binary <" << uiBinary
                     << "> ;\n  rdfs:seeAlso <plugin.ttl> .\n";
        if (!manifest) { std::cerr << "ravet_lv2_turtle: cannot write manifest.ttl\n"; return 1; }
    }

    std::ostringstream out;
    out << "@prefix atom: <http://lv2plug.in/ns/ext/atom#> .\n"
           "@prefix doap: <http://usefulinc.com/ns/doap#> .\n"
           "@prefix foaf: <http://xmlns.com/foaf/0.1/> .\n"
           "@prefix lv2: <http://lv2plug.in/ns/lv2core#> .\n"
           "@prefix midi: <http://lv2plug.in/ns/ext/midi#> .\n"
           "@prefix opts: <http://lv2plug.in/ns/ext/options#> .\n"
           "@prefix pprops: <http://lv2plug.in/ns/ext/port-props#> .\n"
           "@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n"
           "@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n"
           "@prefix state: <http://lv2plug.in/ns/ext/state#> .\n"
           "@prefix time: <http://lv2plug.in/ns/ext/time#> .\n"
           "@prefix ui: <http://lv2plug.in/ns/extensions/ui#> .\n"
           "@prefix units: <http://lv2plug.in/ns/extensions/units#> .\n"
           "@prefix urid: <http://lv2plug.in/ns/ext/urid#> .\n"
           "@prefix bufsz: <http://lv2plug.in/ns/ext/buf-size#> .\n\n";
    out << "<" << uri << ">\n  a lv2:Plugin, doap:Project";
    if (product.category == RAVET_PLUGIN_INSTRUMENT) out << ", lv2:InstrumentPlugin";
    else if (product.category == RAVET_PLUGIN_MIDI_EFFECT) out << ", lv2:MIDIPlugin";
    out << " ;\n  doap:name " << literal(product.name) << " ;\n";
    out << "  doap:maintainer [ foaf:name " << literal(product.vendor ? product.vendor : "");
    if (product.url && *product.url && validIri(product.url)) out << " ; foaf:homepage <" << product.url << ">";
    out << " ] ;\n";
    if (!license.empty()) out << "  doap:license <" << license << "> ;\n";
    out << "  rdfs:comment " << literal(std::string("Version ") + (product.version ? product.version : "")) << " ;\n";
    out << "  lv2:minorVersion " << (major * 1000 + minor) << " ;\n  lv2:microVersion " << patch << " ;\n";
    out << "  lv2:optionalFeature urid:map, opts:options, bufsz:boundedBlockLength ;\n";
    out << "  opts:supportedOption bufsz:maxBlockLength, bufsz:nominalBlockLength ;\n";
    out << "  lv2:extensionData state:interface ;\n";
    if (editor) out << "  ui:ui <" << ui << "> ;\n";

    const auto ports = portLayout(*entry);
    for (size_t index = 0; index < ports.size(); ++index) {
        const auto &port = ports[index];
        out << (index ? " , [\n" : "  lv2:port [\n");
        auto head = [&](const char *types) {
            out << "    a " << types << " ;\n    lv2:index " << index << " ;\n    lv2:symbol " << literal(port.symbol)
                << " ;\n    lv2:name " << literal(port.name);
        };
        switch (port.kind) {
        case PortKind::AudioIn: head("lv2:InputPort, lv2:AudioPort"); break;
        case PortKind::AudioOut: head("lv2:OutputPort, lv2:AudioPort"); break;
        case PortKind::EventsIn:
            head("lv2:InputPort, atom:AtomPort");
            out << " ;\n    atom:bufferType atom:Sequence ;\n    atom:supports time:Position";
            if (product.note_input) out << ", midi:MidiEvent";
            out << " ;\n    lv2:designation lv2:control ;\n    lv2:portProperty lv2:connectionOptional";
            break;
        case PortKind::MidiOut:
            head("lv2:OutputPort, atom:AtomPort");
            out << " ;\n    atom:bufferType atom:Sequence ;\n    atom:supports midi:MidiEvent ;\n"
                   "    lv2:portProperty lv2:connectionOptional";
            break;
        case PortKind::Latency:
            head("lv2:OutputPort, lv2:ControlPort");
            out << " ;\n    lv2:designation lv2:latency ;\n    lv2:minimum 0 ;\n    lv2:maximum 192000 ;\n"
                   "    lv2:portProperty lv2:integer, pprops:notOnGUI ;\n    units:unit units:frame";
            break;
        case PortKind::Parameter:
        case PortKind::ParameterOut:
        case PortKind::Enabled: {
            RavetPluginParameterV1 parameter{};
            entry->parameter(port.parameter, &parameter);
            head(port.kind == PortKind::ParameterOut ? "lv2:OutputPort, lv2:ControlPort" : "lv2:InputPort, lv2:ControlPort");
            if (port.kind == PortKind::Enabled) {
                const bool bypassed = isBypassed(parameter, parameter.default_value);
                out << " ;\n    lv2:designation lv2:enabled ;\n    lv2:default " << (bypassed ? "0" : "1")
                    << " ;\n    lv2:minimum 0 ;\n    lv2:maximum 1 ;\n    lv2:portProperty lv2:toggled, lv2:integer";
                break;
            }
            out << " ;\n    lv2:default " << number(parameter.default_value) << " ;\n    lv2:minimum "
                << number(parameter.minimum) << " ;\n    lv2:maximum " << number(parameter.maximum);
            std::string properties;
            auto add = [&](const char *property) { properties += (properties.empty() ? "" : ", ") + std::string(property); };
            const uint32_t steps = parameter.steps;
            const bool integral = steps && std::trunc(parameter.minimum) == parameter.minimum &&
                                  std::trunc(parameter.maximum) == parameter.maximum &&
                                  parameter.maximum - parameter.minimum == double(steps);
            const bool toggle = steps == 1 && parameter.minimum == 0 && parameter.maximum == 1;
            const bool enumeration = steps > 1 && steps <= 64;
            if (toggle) { add("lv2:toggled"); add("lv2:integer"); }
            else {
                if (integral) add("lv2:integer");
                if (enumeration) add("lv2:enumeration");
            }
            if (parameter.scale == 1 && parameter.minimum > 0) add("pprops:logarithmic");
            if (!(parameter.flags & RAVET_PLUGIN_PARAMETER_AUTOMATABLE) && port.kind == PortKind::Parameter) add("pprops:notAutomatic");
            if (parameter.flags & RAVET_PLUGIN_PARAMETER_HIDDEN) add("pprops:notOnGUI");
            if (!properties.empty()) out << " ;\n    lv2:portProperty " << properties;
            if (steps && !toggle && !integral && !enumeration) out << " ;\n    pprops:rangeSteps " << (steps + 1);
            if (enumeration) {
                out << " ;\n    lv2:scalePoint ";
                for (uint32_t step = 0; step <= steps; ++step) {
                    const double value = parameter.minimum + (parameter.maximum - parameter.minimum) * step / steps;
                    char text[128] = {};
                    if (entry->value_to_text(port.parameter, value, text, sizeof text) != 0 || !*text)
                        std::snprintf(text, sizeof text, "%g", value);
                    out << (step ? ", " : "") << "[ rdfs:label " << literal(text) << " ; rdf:value " << number(value) << " ]";
                }
            }
            const std::string unit = parameter.unit ? parameter.unit : "";
            if (!unit.empty()) {
                const std::string known = unitUri(unit);
                if (!known.empty()) out << " ;\n    units:unit " << known;
                else
                    out << " ;\n    units:unit [ a units:Unit ; rdfs:label " << literal(unit) << " ; units:symbol "
                        << literal(unit) << " ; units:render " << literal("%f " + unit) << " ]";
            }
            break;
        }
        }
        out << "\n  ]";
    }
    out << " .\n";

    if (editor) {
        out << "\n<" << ui << ">\n  a ui:" << uiClass << " ;\n"
            << "  lv2:requiredFeature ui:idleInterface, ui:parent ;\n"
            << "  lv2:optionalFeature ui:resize, ui:touch ;\n"
            << "  lv2:extensionData ui:idleInterface, ui:resize";
        for (const auto &port : ports)
            if (port.kind == PortKind::Parameter || port.kind == PortKind::Enabled || port.kind == PortKind::ParameterOut)
                out << " ;\n  ui:portNotification [ ui:plugin <" << uri << "> ; lv2:symbol " << literal(port.symbol)
                    << " ; ui:protocol ui:floatProtocol ]";
        out << " .\n";
    }

    std::ofstream plugin(directory + "/plugin.ttl");
    plugin << out.str();
    if (!plugin) { std::cerr << "ravet_lv2_turtle: cannot write plugin.ttl\n"; return 1; }
    return 0;
}
