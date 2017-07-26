#pragma once
#include "RestBase.h"

#include <windows.h>
#include <shlwapi.h>
#include <tlhelp32.h>
#include <Psapi.h>
#include <codecvt>
#include <ppl.h>
#include <thread>

#define LOCK(mtx, elem) \
	mtx.lock(); \
	elem; \
	mtx.unlock(); \

class TimeTracking {
	public:
		TimeTracking(RestBase& client);
		
		bool addProcess(const string& process);
		bool deleteProcess(const string& process);
		bool isProcessRunning(const string& process);

		~TimeTracking();
	private:
		map<string, std::pair<bool, bool>> processMap;
		void workerDelegate();

		thread worker;
		RestBase& restClient;
		mutex processesMutex;
};