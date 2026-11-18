#include "bindings/bindings.h"
#import <UIKit/UIKit.h>
#import <objc/runtime.h>

// Die iOS-Formularleiste (∧ ∨ Fertig) über der Tastatur nur in normalen Feldern: Fängt Shell oder Desktop die Tasten ab
// (osk.ts), steht sie nur im Weg, Sesam hat dort eigene Zusatztasten mit «Tastatur ausblenden». Die Seite meldet das
// über den Befehl key_capture (lib.rs → +[SesamKeyCapture setOn:], zur Laufzeit gesucht). WKContentView ist die Eingabeansicht von WKWebView; die
// Kurzbefehlsleiste des iPad (inputAssistantItem) bleibt.
static BOOL keyCapture = NO;
static __weak UIResponder *contentView = nil;

@interface SesamKeyCapture : NSObject
+ (void)setOn:(BOOL)on;
@end
@implementation SesamKeyCapture
+ (void)setOn:(BOOL)on {
	dispatch_async(dispatch_get_main_queue(), ^{
		if (keyCapture == on) return;
		keyCapture = on;
		[contentView reloadInputViews];
	});
}
@end

static void hideFormAccessoryBar(void) {
	Class cls = objc_getClass("WKContentView");
	if (!cls) return;
	SEL sel = @selector(inputAccessoryView);
	Method m = class_getInstanceMethod(cls, sel);
	IMP orig = method_getImplementation(m);
	IMP imp = imp_implementationWithBlock(^UIView *(UIResponder *self) {
		contentView = self;
		return keyCapture ? nil : ((UIView * (*)(id, SEL))orig)(self, sel);
	});
	if (!class_addMethod(cls, sel, imp, method_getTypeEncoding(m))) method_setImplementation(m, imp);
}

int main(int argc, char * argv[]) {
	hideFormAccessoryBar();
	ffi::start_app();
	return 0;
}
