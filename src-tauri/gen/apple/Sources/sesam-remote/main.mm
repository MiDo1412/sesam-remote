#include "bindings/bindings.h"
#import <UIKit/UIKit.h>
#import <objc/runtime.h>

// Ohne die iOS-Formularleiste (∧ ∨ ✓) über der Tastatur: In der Shell und im Desktop steht sie nur im Weg,
// Sesam hat eigene Zusatztasten. WKContentView ist die Eingabeansicht von WKWebView; die Kurzbefehlsleiste
// des iPad (inputAssistantItem) bleibt.
static void hideFormAccessoryBar(void) {
	Class cls = objc_getClass("WKContentView");
	if (!cls) return;
	IMP none = imp_implementationWithBlock(^UIView *(id self) { return nil; });
	Method m = class_getInstanceMethod(cls, @selector(inputAccessoryView));
	if (!class_addMethod(cls, @selector(inputAccessoryView), none, method_getTypeEncoding(m))) method_setImplementation(m, none);
}

int main(int argc, char * argv[]) {
	hideFormAccessoryBar();
	ffi::start_app();
	return 0;
}
