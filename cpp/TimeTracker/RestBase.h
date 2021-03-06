#pragma once
#include <string>
#include <cpprest/json.h>
#include <cpprest/http_client.h>
#include <iostream>
#include <fstream>

using namespace std;
using namespace web;
using namespace web::http;
using namespace web::http::client;
using namespace web::json;
using namespace utility;

typedef struct tm st_time;

class RestBase {
	public: 
		RestBase(const string& baseUrl);
		RestBase(const string& baseUrl, const string& authentication);

		enum class RECIEVE_TYPES {LONGEST_SESSION, DURATION, LAUNCHES, TIMELINE};

		const string& getBaseUrl();
		const string& getAuthenticationString();
		bool getHasAuthentication();

		const st_time getStartTime();
		void setStartTime(const st_time& startTime);

		virtual json::value getData(const string& searchQuery) = 0;
		virtual bool deleteData(const string& appName) = 0;
		virtual bool putData(const string& appName) = 0;
		virtual bool patchData(const string& appName, const string& key, const json::value& value) = 0;
		virtual vector<string> getProcesses() = 0;

		virtual bool onDataRecieve(RECIEVE_TYPES type, const string& process, int data = 1) = 0;

		static const string readConfig(const string& arg);

	protected:
		json::value getDataBase(uri_builder& builder);
		bool deleteDataBase(uri_builder& builder);
		bool putDataBase(uri_builder& builder, const json::value& content);
		bool patchDataBase(uri_builder& builder, const json::value& content);

	private:
		const string baseUrl;
		const string authentication;
		bool hasAuthentication;
		http_client httpClient;
		st_time startTime;
};