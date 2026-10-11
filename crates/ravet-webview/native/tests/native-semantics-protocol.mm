#import <Cocoa/Cocoa.h>
#include "../../../ravet-surface/native/RavetWebView.h"
#include <iostream>
#include <stdexcept>
#include <thread>
extern "C" bool w223_test_exhaust_revision(void*);
struct State {
    void* view=nullptr;
    id element=nil;
    int calls=0;
    bool accept=true,reenter=false,install=false,destroy=false,hide=false;
    uint64_t id=0,revision=0;
    uint32_t kind=99,status=99;
};
static void require(bool value,const char* text) { if(!value)throw std::runtime_error(text); }
static bool action(void* raw,uint64_t id,uint64_t revision,uint32_t kind) {
    State* state=static_cast<State*>(raw);++state->calls;state->id=id;state->revision=revision;state->kind=kind;
    if(state->reenter)require(![state->element accessibilityPerformPress],"Nested action refused");
    if(state->install) { uint64_t next=0;state->status=ravet_web_accessibility_set(state->view,nullptr,nullptr,0,0,nullptr,nullptr,&next); }
    if(state->hide)ravet_web_set_visible(state->view,false);
    if(state->destroy) { ravet_web_destroy(state->view);state->view=nullptr; }
    return state->accept;
}
static int groups=0;
static void pass(const char* text) { ++groups;std::cout<<text<<" PASS\n"; }
int main() {@autoreleasepool {
    NSWindow* window=nil;State state;
    try {
        [NSApplication sharedApplication];[NSApp setActivationPolicy:NSApplicationActivationPolicyAccessory];[NSApp finishLaunching];
        window=[[NSWindow alloc] initWithContentRect:NSMakeRect(140,140,700,500) styleMask:NSWindowStyleMaskTitled backing:NSBackingStoreBuffered defer:NO];
        window.title=@"RAVET owned native companion protocol fixture";window.releasedWhenClosed=NO;
        state.view=ravet_web_create_html("<body>Custom editor fixture</body>",700,500,nullptr,nullptr,nullptr);
        require(state.view,"Create reusable view");NSView* root=(__bridge NSView*)state.view;
        ravet_web_attach(state.view,(__bridge void*)window.contentView);[window orderFront:nil];
        NSArray* original=[root accessibilityChildren];uint64_t revision=0;
        require(ravet_web_accessibility_set(state.view,nullptr,nullptr,0,0,nullptr,nullptr,&revision)==0,"Default clear");
        require([[root accessibilityChildren] isEqual:original] || (![root accessibilityChildren]&&!original),"Default original traversal");pass("None preserves actual original native traversal");
        RavetAccessibilityNode nodes[]={
            {1,"Output status","Ready",20,20,180,24,0,0,0,0,0,true,false},
            {2,"Gain",nullptr,20,60,180,24,0,.5,1,.1,2,true,false},
            {3,"Reset gain",nullptr,20,100,180,24,0,0,0,0,1,true,false}
        };
        require(ravet_web_accessibility_set(state.view,"Custom output",nodes,3,2,&state,action,&revision)==0,"Install complete snapshot");
        NSArray* children=[root accessibilityChildren];require(children.count==3,"Three native children");
        id text=children[0],slider=children[1],button=children[2];state.element=button;
        require([[root accessibilityRole] isEqual:NSAccessibilityGroupRole]&&[[root accessibilityLabel] isEqual:@"Custom output"],"Native group");
        require([[text accessibilityRole] isEqual:NSAccessibilityStaticTextRole]&&[[text accessibilityValue] isEqual:@"Ready"],"Native static text");
        require([[slider accessibilityRole] isEqual:NSAccessibilitySliderRole]&&[[slider accessibilityLabel] isEqual:@"Gain"]&&[[slider accessibilityValue] doubleValue]==.5,"Native slider");
        require([[button accessibilityRole] isEqual:NSAccessibilityButtonRole]&&[[button accessibilityLabel] isEqual:@"Reset gain"],"Native button");pass("Real native protocol group roles labels and typed values");
        require([slider accessibilityParent]==root&&[[slider accessibilityMinValue] doubleValue]==0&&[[slider accessibilityMaxValue] doubleValue]==1,"Parent and bounds");
        require([[root accessibilityChildrenInNavigationOrder] isEqual:children]&&[root accessibilityFocusedUIElement]==slider&&[slider isAccessibilityFocused],"Navigation and focus");pass("Native parent bounds navigation and reported focus");
        NSRect local=NSMakeRect(20,60,180,24);NSRect expected=[window convertRectToScreen:[root convertRect:local toView:nil]];
        require(NSEqualRects([slider accessibilityFrame],expected),"Independent frame conversion");
        NSPoint point=NSMakePoint(NSMidX(expected),NSMidY(expected));require([root accessibilityHitTest:point]==slider,"Native hit test");
        NSRect before=[slider accessibilityFrame];[window setFrameOrigin:NSMakePoint(240,240)];NSRect after=[slider accessibilityFrame];
        require(after.origin.x-before.origin.x==100&&after.origin.y-before.origin.y==100,"Frame tracks owned window");pass("Native screen frame hit test and own window movement");
        require([button accessibilityPerformPress]&&state.calls==1&&state.id==3&&state.revision==revision&&state.kind==0,"Exact press intent");pass("Native press routes exact bounded intent");
        state.accept=false;require(![button accessibilityPerformPress]&&state.calls==2,"Refused press");state.accept=true;
        require([[slider accessibilityValue] doubleValue]==.5,"No implicit model mutation");pass("Refused intent and no implicit native value edit");
        require([slider accessibilityPerformIncrement]&&state.kind==1,"Increment");require([slider accessibilityPerformDecrement]&&state.kind==2,"Decrement");pass("Native discrete slider intents");
        int calls=state.calls;require(![slider accessibilityPerformPress]&&![button accessibilityPerformIncrement]&&![text accessibilityPerformPress]&&state.calls==calls,"Wrong roles refuse");
        require(![slider isAccessibilitySelectorAllowed:@selector(setAccessibilityValue:)]&&![button isAccessibilitySelectorAllowed:@selector(accessibilityPerformIncrement)],"Selector profile");pass("Role actions and value setter admission");
        state.reenter=true;state.install=true;require([button accessibilityPerformPress]&&state.calls==calls+1&&state.status==3,"Busy callback admission");state.reenter=false;state.install=false;pass("Native callback nested action and install guard");
        nodes[1].value=1;nodes[2].enabled=false;nodes[0].hidden=true;
        require(ravet_web_accessibility_set(state.view,"Custom output",nodes,3,2,&state,action,&revision)==0,"New snapshot");
        NSArray* updated=[root accessibilityChildren];require(updated.count==2,"Hidden excludes child");id endpoint=updated[0],disabled=updated[1];calls=state.calls;
        require(![endpoint accessibilityPerformIncrement]&&![disabled accessibilityPerformPress]&&![disabled isAccessibilityEnabled]&&state.calls==calls,"Endpoint and disabled refuse");pass("Hidden disabled and slider endpoint profile");
        require([[button accessibilityLabel] isEqual:@"Reset gain"]&&![button accessibilityParent]&&NSEqualRects([button accessibilityFrame],NSZeroRect)&&![button isAccessibilityFocused]&&![button accessibilityPerformPress],"Stale copied metadata and inert ownership");pass("Retained old native element cannot regain ownership");
        uint64_t output=999;nodes[1].value=2;
        require(ravet_web_accessibility_set(state.view,"Custom output",nodes,3,2,&state,action,&output)==1&&output==999&&[[root accessibilityChildren] isEqual:updated],"Whole failure preserves native tree/output");nodes[1].value=1;pass("Complete native preflight preserves prior hierarchy");
        ravet_web_set_visible(state.view,false);require(![root accessibilityChildren].count&&![endpoint accessibilityPerformDecrement],"Hidden owner");ravet_web_set_visible(state.view,true);require([root accessibilityChildren].count==2,"Reveal admitted hierarchy");pass("Original native visibility admission");
        ravet_web_detach(state.view);require(![root accessibilityChildren].count&&![endpoint accessibilityPerformDecrement],"Detached owner");
        NSView* other=[[NSView alloc] initWithFrame:window.contentView.bounds];[window.contentView addSubview:other];ravet_web_attach(state.view,(__bridge void*)other);
        require(![endpoint accessibilityPerformDecrement],"Reparent cannot revive original element");pass("Detached and foreign parent reject native actions");
        require(ravet_web_accessibility_set(state.view,nullptr,nullptr,0,0,nullptr,nullptr,&revision)==0&&![endpoint accessibilityParent],"Cleanup after retirement");pass("None cleanup restores original traversal after reparent");
        ravet_web_detach(state.view);ravet_web_attach(state.view,(__bridge void*)window.contentView);nodes[0].hidden=false;nodes[2].enabled=true;nodes[1].value=.5;
        require(ravet_web_accessibility_set(state.view,"Custom output",nodes,3,3,&state,action,&revision)==0,"Fresh owner install");
        button=[root accessibilityChildren][2];state.element=button;
        bool workerInert=false;
        std::thread worker([&]{@autoreleasepool { workerInert=![button accessibilityParent]&&![button accessibilityPerformPress]&&![root accessibilityChildren].count; }});worker.join();require(workerInert,"Worker protocol refusal");pass("Off-main-thread protocol refuses mutable owner reads");
        state.hide=true;calls=state.calls;require([button accessibilityPerformPress]&&state.calls==calls+1&&![button accessibilityPerformPress]&&![root accessibilityChildren].count,"Accepted callback retirement cannot revive owner");state.hide=false;ravet_web_set_visible(state.view,true);pass("Accepted intent retains no stale presentation after callback hide");
        require(w223_test_exhaust_revision(state.view),"Private revision boundary injection");uint64_t exhaustedOutput=123;
        require(ravet_web_accessibility_set(state.view,"Custom output",nodes,3,3,&state,action,&exhaustedOutput)==4&&exhaustedOutput==123&&[[root accessibilityChildren] count]==3,"Revision exhaustion preflight");pass("Native revision exhaustion preserves tree and caller output");
        // Cleanup remains possible at exhaustion; it cannot create a new revision.
        require(ravet_web_accessibility_set(state.view,nullptr,nullptr,0,0,nullptr,nullptr,&exhaustedOutput)==0&&exhaustedOutput==UINT64_MAX&&![button accessibilityPerformPress],"Exhausted original-owner cleanup");pass("Exhausted native owner still permits complete cleanup");
        ravet_web_destroy(state.view);state.view=ravet_web_create_html("<body>Teardown fixture</body>",700,500,nullptr,nullptr,nullptr);root=(__bridge NSView*)state.view;ravet_web_attach(state.view,(__bridge void*)window.contentView);
        require(ravet_web_accessibility_set(state.view,"Custom output",nodes,3,3,&state,action,&revision)==0,"Fresh teardown owner");button=[root accessibilityChildren][2];state.element=button;
        state.destroy=true;require([button accessibilityPerformPress]&&!state.view&&![button accessibilityPerformPress]&&![button accessibilityParent],"Callback teardown without stale native pointer");pass("Native callback teardown retains only inert element metadata");
        [window orderOut:nil];[window close];window=nil;
        require(groups==20,"Expected native groups");std::cout<<groups<<" native protocol groups PASS\n";return 0;
    } catch(const std::exception& error) {
        std::cerr<<error.what()<<'\n';if(state.view)ravet_web_destroy(state.view);if(window){[window orderOut:nil];[window close];}return 1;
    }
}}
