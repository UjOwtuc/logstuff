#include <QApplication>

#include "mainwindow.h"

int main(int argc, char *argv[])
{
	QApplication app(argc, argv);
	QCoreApplication::setOrganizationName("kbo");
	QCoreApplication::setApplicationName("qstuff");

	QStuffMainWindow* mw = new QStuffMainWindow;
	mw->show();

	return app.exec();
}
