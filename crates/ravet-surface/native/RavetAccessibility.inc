// Explicit opt-in content semantics. No DOM scraping or remote WebKit AX access.
#include <cstdint>
#include <limits>

@class RavetAccessibilityOwner;
@interface RavetAccessibilityElement : NSAccessibilityElement
@property(nonatomic,weak) RavetAccessibilityOwner* owner;
@property(nonatomic) uint64_t nodeID;
@property(nonatomic) uint64_t revision;
@property(nonatomic) uint32_t kind;
@property(nonatomic,copy) NSString* label;
@property(nonatomic,copy) NSString* text;
@property(nonatomic) NSRect localFrame;
@property(nonatomic) double minimum,value,maximum,increment;
@property(nonatomic) BOOL enabled,hidden;
-(BOOL)live;
-(BOOL)act:(uint32_t)kind;
@end
@interface RavetAccessibilityButton : RavetAccessibilityElement<NSAccessibilityButton>
@end
@interface RavetAccessibilitySlider : RavetAccessibilityElement<NSAccessibilitySlider>
@end
@interface RavetAccessibilityText : RavetAccessibilityElement<NSAccessibilityStaticText>
@end
@interface RavetAccessibilityOwner : NSObject
@property(nonatomic,weak) RavetWebContainer* root;
@property(nonatomic,weak) NSView* parent;
@property(nonatomic,weak) NSWindow* window;
@property(nonatomic,copy) NSString* label;
@property(nonatomic,copy) NSArray<RavetAccessibilityElement*>* elements;
@property(nonatomic) uint64_t revision,focused;
@property(nonatomic) BOOL retired,busy;
@property(nonatomic) void* context;
@property(nonatomic) RavetAccessibilityAction action;
-(BOOL)owned;
-(BOOL)visible;
-(NSArray*)children;
-(id)focusedElement;
-(void)retire;
@end
@interface NSWindow(RavetAccessibilityLifecycle)
-(BOOL)ravetIsClosed;
@end
static bool ravetAccessibilityWindowClosed(NSWindow* window) {
    return [window respondsToSelector:@selector(ravetIsClosed)] && [(id)window ravetIsClosed];
}
@implementation RavetAccessibilityOwner
-(BOOL)owned {
    if(!NSThread.isMainThread||_retired)return NO;
    RavetWebContainer* root=_root;NSView* parent=_parent;NSWindow* window=_window;
    return root && parent && window && root.accessibilityOwner==self &&
        root.superview==parent && root.window==window && parent.window==window &&
        !ravetAccessibilityWindowClosed(window);
}
-(BOOL)visible {
    if(![self owned] || !_window.visible)return NO;
    NSView* view=_root;
    for(unsigned depth=0;view && depth<32;++depth) {
        if(view.hidden)return NO;
        view=view.superview;
    }
    return !view && [self owned];
}
-(NSArray*)children {
    if(![self visible])return @[];
    NSMutableArray* result=[NSMutableArray arrayWithCapacity:_elements.count];
    for(RavetAccessibilityElement* element in _elements)if(!element.hidden && [element live])[result addObject:element];
    return result;
}
-(id)focusedElement {
    if(![self visible])return nil;
    for(RavetAccessibilityElement* element in _elements)
        if(element.nodeID==_focused && element.enabled && !element.hidden && [element live])return element;
    return nil;
}
-(void)retire { _retired=YES;_action=nullptr;_context=nullptr; }
@end
@implementation RavetAccessibilityElement
-(BOOL)live {
    if(!NSThread.isMainThread)return NO;
    RavetAccessibilityOwner* owner=_owner;
    return owner && owner.revision==_revision && [owner owned];
}
-(BOOL)isAccessibilityElement { return [self live] && !_hidden; }
-(NSString*)accessibilityIdentifier { return [NSString stringWithFormat:@"ravet-%llu-%llu",(unsigned long long)_revision,(unsigned long long)_nodeID]; }
-(NSAccessibilityRole)accessibilityRole {
    return _kind==1?NSAccessibilityButtonRole:(_kind==2?NSAccessibilitySliderRole:NSAccessibilityStaticTextRole);
}
-(NSString*)accessibilityLabel { return _label; }
-(id)accessibilityValue { return _kind==2?@(_value):(_kind==0?_text:nil); }
-(id)accessibilityMinValue { return _kind==2?@(_minimum):nil; }
-(id)accessibilityMaxValue { return _kind==2?@(_maximum):nil; }
-(BOOL)isAccessibilityEnabled { return [self live] && _enabled && !_hidden && [_owner visible]; }
-(BOOL)isAccessibilityFocused { return [self isAccessibilityEnabled] && _owner.focused==_nodeID; }
-(id)accessibilityParent { return [self live]?_owner.root:nil; }
-(NSRect)accessibilityFrame {
    if(![self live]||![_owner visible])return NSZeroRect;
    return NSAccessibilityFrameInView(_owner.root,_localFrame);
}
-(BOOL)isAccessibilitySelectorAllowed:(SEL)selector {
    NSString* name=NSStringFromSelector(selector);
    if([name hasPrefix:@"set"])return NO;
    if(selector==@selector(accessibilityPerformPress))return _kind==1;
    if(selector==@selector(accessibilityPerformIncrement)||selector==@selector(accessibilityPerformDecrement))return _kind==2;
    if([name hasPrefix:@"accessibilityPerform"])return NO;
    return [super isAccessibilitySelectorAllowed:selector];
}
-(BOOL)act:(uint32_t)kind {
    if(![self isAccessibilityEnabled])return NO;
    RavetAccessibilityOwner* owner __attribute__((objc_precise_lifetime))=_owner;
    if(!owner||owner.busy||!owner.action||!owner.context)return NO;
    if((kind==0&&_kind!=1)||(kind!=0&&_kind!=2)||kind>2)return NO;
    if((kind==1&&_value>=_maximum)||(kind==2&&_value<=_minimum))return NO;
    owner.busy=YES;
    @try {
        return owner.action(owner.context,_nodeID,_revision,kind);
    } @finally { owner.busy=NO; }
}
-(BOOL)accessibilityPerformPress { return [self act:0]; }
-(BOOL)accessibilityPerformIncrement { return [self act:1]; }
-(BOOL)accessibilityPerformDecrement { return [self act:2]; }
@end
// Role protocols require explicit concrete overrides, even with superclass implementations.
#define RAVET_AX_COMMON \
-(NSRect)accessibilityFrame { return [super accessibilityFrame]; } \
-(id)accessibilityParent { return [super accessibilityParent]; }
@implementation RavetAccessibilityButton
RAVET_AX_COMMON
-(NSString*)accessibilityLabel { return [super accessibilityLabel]; }
-(BOOL)accessibilityPerformPress { return [super accessibilityPerformPress]; }
@end
@implementation RavetAccessibilitySlider
RAVET_AX_COMMON
-(NSString*)accessibilityLabel { return [super accessibilityLabel]; }
-(id)accessibilityValue { return [super accessibilityValue]; }
-(BOOL)accessibilityPerformIncrement { return [super accessibilityPerformIncrement]; }
-(BOOL)accessibilityPerformDecrement { return [super accessibilityPerformDecrement]; }
@end
@implementation RavetAccessibilityText
RAVET_AX_COMMON
-(NSString*)accessibilityValue { return [super accessibilityValue]; }
@end
#undef RAVET_AX_COMMON

static bool ravetAccessibilityString(const char* text,size_t minimum,size_t maximum,NSString** result,size_t* total) {
    if(!text)return false;const size_t length=strnlen(text,maximum+1);
    if(length<minimum||length>maximum||*total>65536-length)return false;
    NSString* value=[[NSString alloc] initWithBytes:text length:length encoding:NSUTF8StringEncoding];
    if(!value)return false;*result=value;*total+=length;return true;
}
static bool ravetAccessibilityFrame(const RavetAccessibilityNode& n) {
    for(double v:{n.x,n.y,n.width,n.height,n.x+n.width,n.y+n.height})
        if(!std::isfinite(v)||std::abs(v)>16384)return false;
    return n.width>0&&n.height>0;
}
// Status0 accepted,1 invalid,2 unavailable,3 busy,4 exhausted,5 wrong-thread.
extern "C" uint32_t ravet_web_accessibility_set(void* view,const char* label,
    const RavetAccessibilityNode* nodes,size_t count,uint64_t focused,void* context,
    RavetAccessibilityAction action,uint64_t* revision) {
    if(!NSThread.isMainThread)return 5;
    if(!view||!revision)return 1;
    RavetWebContainer* root __attribute__((objc_precise_lifetime))=(__bridge RavetWebContainer*)view;
    if(root.accessibilityTransition||root.accessibilityOwner.busy)return 3;
    if(label&&root.accessibilityRevision==std::numeric_limits<uint64_t>::max())return 4;
    const uint64_t next=root.accessibilityRevision==std::numeric_limits<uint64_t>::max()?root.accessibilityRevision:root.accessibilityRevision+1;
    // None is cleanup, including original-owner detach/retirement.
    if(!label) {
        if(nodes||count||focused||context||action)return 1;
        root.accessibilityTransition=YES;
        @try {
            [root.accessibilityOwner retire];root.accessibilityOwner=nil;root.accessibilityRevision=next;
            *revision=next;
            if(root.window.visible)NSAccessibilityPostNotification(root,NSAccessibilityLayoutChangedNotification);
        } @finally { root.accessibilityTransition=NO; }
        return 0;
    }
    if(!root.superview||!root.window||ravetAccessibilityWindowClosed(root.window))return 2;
    if(!nodes||!count||count>64||!context||!action)return 1;
    size_t total=0;NSString* panel=nil;
    if(!ravetAccessibilityString(label,1,1024,&panel,&total))return 1;
    NSMutableArray* children=[NSMutableArray arrayWithCapacity:count];
    bool focusValid=!focused;
    RavetAccessibilityOwner* owner=[RavetAccessibilityOwner new];
    owner.root=root;owner.parent=root.superview;owner.window=root.window;owner.label=panel;
    owner.revision=next;owner.focused=focused;owner.context=context;owner.action=action;
    for(size_t index=0;index<count;++index) {
        const RavetAccessibilityNode& n=nodes[index];NSString* name=nil,*text=nil;
        bool duplicate=false;for(size_t prior=0;prior<index;++prior)if(nodes[prior].id==n.id)duplicate=true;
        if(!n.id||duplicate||n.kind>2||!ravetAccessibilityFrame(n)||
            !ravetAccessibilityString(n.label,1,1024,&name,&total))return 1;
        if(n.kind==0) { if(!ravetAccessibilityString(n.text,0,4096,&text,&total))return 1; }
        else if(n.text)return 1;
        if(n.kind==2) {
            for(double value:{n.minimum,n.value,n.maximum,n.increment})if(!std::isfinite(value))return 1;
            if(n.minimum>n.value||n.value>n.maximum||n.increment<=0)return 1;
        }
        if(n.id==focused&&n.kind!=0&&n.enabled&&!n.hidden)focusValid=true;
        RavetAccessibilityElement* element=n.kind==0?[RavetAccessibilityText new]:
            (n.kind==1?[RavetAccessibilityButton new]:[RavetAccessibilitySlider new]);
        element.owner=owner;element.nodeID=n.id;element.revision=next;element.kind=n.kind;
        element.label=name;element.text=text;element.localFrame=NSMakeRect(n.x,n.y,n.width,n.height);
        element.enabled=n.enabled;element.hidden=n.hidden;element.minimum=n.minimum;
        element.value=n.value;element.maximum=n.maximum;element.increment=n.increment;
        [children addObject:element];
    }
    if(!focusValid)return 1;
    owner.elements=children;
    if(root.accessibilityTransition||root.accessibilityOwner.busy)return 3;
    if(owner.parent!=root.superview||owner.window!=root.window||ravetAccessibilityWindowClosed(root.window))return 2;
    root.accessibilityTransition=YES;owner.busy=YES;
    @try {
        [root.accessibilityOwner retire];root.accessibilityOwner=owner;root.accessibilityRevision=next;
        *revision=next;
        if([owner visible]) {
            NSAccessibilityPostNotification(root,NSAccessibilityLayoutChangedNotification);
            for(RavetAccessibilityElement* element in owner.elements) {
                if(![owner visible])break;
                if(!element.hidden&&element.kind!=1)NSAccessibilityPostNotification(element,NSAccessibilityValueChangedNotification);
            }
            id focus=[owner focusedElement];
            if(focus&&[owner visible])NSAccessibilityPostNotification(focus,NSAccessibilityFocusedUIElementChangedNotification);
        }
    } @finally { owner.busy=NO;root.accessibilityTransition=NO; }
    return 0;
}

#if defined(RAVET_ACCESSIBILITY_TESTING)
// Private fixture seam; excluded from normal builds. No OS accessibility API.
extern "C" bool w223_test_exhaust_revision(void* view) {
    if(!NSThread.isMainThread||!view)return false;
    RavetWebContainer* root=(__bridge RavetWebContainer*)view;
    if(root.accessibilityTransition||root.accessibilityOwner.busy)return false;
    root.accessibilityRevision=std::numeric_limits<uint64_t>::max();return true;
}
#endif
