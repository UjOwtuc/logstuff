#ifndef SAVEVIEWDIALOG_H
#define SAVEVIEWDIALOG_H

#include <QDialog>

namespace Ui {
	class SaveViewDialog;
}

class SaveViewDialog : public QDialog
{
public:
	explicit SaveViewDialog(QWidget* parent = nullptr, Qt::WindowFlags f = Qt::WindowFlags());
	QString name() const;
	bool saveQuery() const;
	bool saveTimerange() const;

private:
	Ui::SaveViewDialog* m_widget;
};

#endif // SAVEVIEWDIALOG_H
