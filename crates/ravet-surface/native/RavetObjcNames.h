#pragma once
// Objective-C classes share one flat namespace per process, so a RAVET app
// hosting a RAVET plugin would load two copies of each class below and the
// runtime would pick one at random. Every binary that compiles these sources
// passes its own RAVET_OBJC_SUFFIX (build.rs and CMakeLists.txt derive it
// from the build tree), giving each copy distinct class names.
#ifdef RAVET_OBJC_SUFFIX
#define RAVET_OBJC_PASTE2(name, suffix) name##_##suffix
#define RAVET_OBJC_PASTE(name, suffix) RAVET_OBJC_PASTE2(name, suffix)
#define RAVET_OBJC_CLASS(name) RAVET_OBJC_PASTE(name, RAVET_OBJC_SUFFIX)
#define RavetWebBridge RAVET_OBJC_CLASS(RavetWebBridge)
#define RavetEditorWebView RAVET_OBJC_CLASS(RavetEditorWebView)
#define RavetWebContainer RAVET_OBJC_CLASS(RavetWebContainer)
#define RavetAccessibilityElement RAVET_OBJC_CLASS(RavetAccessibilityElement)
#define RavetAccessibilityButton RAVET_OBJC_CLASS(RavetAccessibilityButton)
#define RavetAccessibilitySlider RAVET_OBJC_CLASS(RavetAccessibilitySlider)
#define RavetAccessibilityText RAVET_OBJC_CLASS(RavetAccessibilityText)
#define RavetAccessibilityOwner RAVET_OBJC_CLASS(RavetAccessibilityOwner)
#define RavetAccessibilityLifecycle RAVET_OBJC_CLASS(RavetAccessibilityLifecycle)
#endif
