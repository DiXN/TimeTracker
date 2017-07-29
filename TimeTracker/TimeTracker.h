#pragma once

#include "FireBaseClient.h"
#include "RestBase.h"
#include "TimeTracking.h"

#ifdef TIMETRACKER_EXPORTS  
	#define EXPORT extern "C" __declspec(dllexport)   
#else  
	#define TIMETRACKER_EXPORTS extern "C"  __declspec(dllimport)   
#endif 


EXPORT void init();
EXPORT bool addProcess(const char* processName);
EXPORT bool deleteProcess(const char* processName);

EXPORT void csharpOnDataChange(string data);

