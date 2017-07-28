#include "stdafx.h"
#include "Exporter.h"

TimeTracking* tracking;

void init() {
	FirebaseClient fireBaseClient("", "");
	tracking = new TimeTracking(fireBaseClient);
}

bool addProcess(const char* processName) {
	return tracking->addProcess(processName);
}

bool deleteProcess(const char* processName) {
	return tracking->deleteProcess(processName);
}


