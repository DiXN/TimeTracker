#include "stdafx.h"
#include "FireBaseClient.h"
#include "RestBase.h"

int main(int argc, char * argv[])
{
	auto today = chrono::system_clock::now();
	time_t currentTime = chrono::system_clock::to_time_t(today);

	FirebaseClient fireBaseClient("", "");
	fireBaseClient.setStartTime(*localtime(&currentTime));

	TimeTracking tracking(fireBaseClient);
}