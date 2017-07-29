#include "stdafx.h"
#include "TimeTracker.h"

RestBase* restClient;
TimeTracking* tracking;

bool isInitialized = false;

typedef int(__stdcall* Callback)(const char* text);

Callback _handler = 0;

void init() {
	if (!isInitialized) {
		restClient = new FirebaseClient("", "");
		tracking = new TimeTracking(*restClient);
		isInitialized = true;
	}
}

bool addProcess(const char* processName) {
	init();
	return tracking->addProcess(processName);
}

bool deleteProcess(const char* processName) {
	init();
	return tracking->deleteProcess(processName);
}

EXPORT void setCsharpCallback(Callback handler) {
	init();
	_handler = handler;
}

void csharpOnDataChange(string data) {
	_handler(data.c_str());
}