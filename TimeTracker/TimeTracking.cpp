#include "stdafx.h"
#include "TimeTracking.h"

void TimeTracking::workerDelegate() {
	concurrency::task_group tasks;
	concurrency::parallel_invoke([&] {
		while (true) {
			for (const auto& process : processMap) {
				if (isProcessRunning(process.first + ".exe")) {
					LOCK(processesMutex, processMap[process.first].first = true);

					auto lockCheck = [&] {
						bool returnVal;
						LOCK(processesMutex, returnVal = processMap[process.first].first && processMap[process.first].second);
						return returnVal;
					};

					if (lockCheck()) {
						tasks.run([&] {
							LOCK(processesMutex, processMap[process.first].second = false)

							LOCK(onDataMutex, restClient.onDataRecieve(RestBase::RECIEVE_TYPES::LAUNCHES, process.first))

							auto start = chrono::high_resolution_clock::now();
							
							auto today = chrono::system_clock::now();
							time_t currentTime = chrono::system_clock::to_time_t(today);
							struct tm *time = localtime(&currentTime);

							auto lockCheck = [&] {
								bool returnVal;
								LOCK(processesMutex, returnVal = processMap[process.first].first);
								return returnVal;
							};

							while (lockCheck()) {
								chrono::duration<float> elapsed = chrono::high_resolution_clock::now() - start;
								if (int(elapsed.count()) != 0 && int(elapsed.count()) % 60 == 0) {
									LOCK(onDataMutex, restClient.onDataRecieve(RestBase::RECIEVE_TYPES::DURATION, process.first))
									LOCK(onDataMutex, restClient.onDataRecieve(RestBase::RECIEVE_TYPES::TIMELINE, process.first))
								}

								pplx::wait(1000);
							}

							chrono::duration<float> end = chrono::high_resolution_clock::now() - start;

							LOCK(onDataMutex, restClient.onDataRecieve(
								RestBase::RECIEVE_TYPES::LONGEST_SESSION, process.first, int(end.count() / 60)))
							LOCK(processesMutex, processMap[process.first].second = true);
						});
					}
				}
				else {
					LOCK(processesMutex, processMap[process.first].first = false);
				}
			}

			pplx::wait(10000);
		}
	},
		[&] { tasks.run([] {});
	});
}

TimeTracking::TimeTracking(RestBase& restClient) : restClient(restClient), worker(thread([=] { workerDelegate(); })) {
	const vector<string>& processes = restClient.getProcesses();

	for (const string& process : processes) {
		processMap.emplace(process, make_pair(true, true));
	}
}

TimeTracking::~TimeTracking() {
	worker.join();
}

bool TimeTracking::addProcess(const string& process) {
	if (processMap.count(process) == 0) {
		if (restClient.putData(process)) {
			processMap.emplace(process, make_pair(true, true));
			return true;
		}
		else {
			return false;
		}
	}
	else {
		return false;
	}
}

bool TimeTracking::deleteProcess(const string& process) {
	if (processMap.count(process) > 0) {
		if (restClient.deleteData(process)) {
			processMap.emplace(process, make_pair(true, true));
			return true;
		}
		else {
			return false;
		}
	}
	else {
		return false;
	}
}

bool TimeTracking::isProcessRunning(const string& process) {
	PROCESSENTRY32 pe;
	HANDLE handle = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
	pe.dwSize = sizeof(PROCESSENTRY32);

	if (!Process32First(handle, &pe)) {
		CloseHandle(handle);
		return false;
	}

	do {
		if (!lstrcmpi(pe.szExeFile, conversions::to_string_t(process).c_str())) {
			CloseHandle(handle);
			return true;
		}

	} while (Process32Next(handle, &pe));

	CloseHandle(handle);

	return false;
}