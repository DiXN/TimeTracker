#include "stdafx.h"
#include "TimeTracker.h"

RestBase* restClient;
TimeTracking* tracking;

bool isInitialized = false;

typedef int(__stdcall* Callback)(const char* text);

Callback _handler = 0;

void init() {
	if (!isInitialized) {
		auto today = chrono::system_clock::now();
		time_t currentTime = chrono::system_clock::to_time_t(today);

		restClient = new FirebaseClient("", "");

		restClient->setStartTime(*localtime(&currentTime));

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

EXPORT void onDataChangeCallback(Callback handler) {
	init();
	_handler = handler;
}

void csharpOnDataChange(string data) {
	_handler(data.c_str());
}

LPWSTR getProcesses() {
	init();
	const vector<string>& processes = restClient->getProcesses();

	json::value processesJson;
	processesJson[U("processes")] = json::value::array(processes.size());

	for (size_t i = 0; i < processes.size(); i++) {
		processesJson[U("processes")][i] = json::value(conversions::to_string_t(processes[i]));
	}

	return ::SysAllocString(processesJson.serialize().c_str());
}
