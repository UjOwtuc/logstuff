#include "saveviewdialog.h"

#include "ui_saveviewdialog.h"

SaveViewDialog::SaveViewDialog(QWidget* parent, Qt::WindowFlags f)
	: QDialog(parent, f)
{
	m_widget = new Ui::SaveViewDialog;
	m_widget->setupUi(this);
}


QString SaveViewDialog::name() const
{
	return m_widget->nameEdit->text();
}


bool SaveViewDialog::saveQuery() const
{
	return m_widget->saveQueryCheckbox->isChecked();
}


bool SaveViewDialog::saveTimerange() const
{
	return m_widget->saveTimerangeCheckbox->isChecked();
}
